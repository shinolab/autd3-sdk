use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Flags, Kind};
use autd3_rs_core::{DeviceStatus, MAX_DEVICES};
use zerocopy::{FromZeros, IntoBytes};

use crate::datagram::Frame;
use crate::device_set::DeviceSet;
use crate::error::{Error, NetworkCause};
use crate::geometry::Geometry;
use crate::protocol::{FrameHeader, MAX_INFLIGHT};
use crate::response::Response;
use crate::udp::{Channel, DeviceReply, Polled, Reply, TransportOption, UdpError};
use crate::value::SysTime;

use super::completion::ResponseSender;
use super::device_clock::DeviceClock;
use super::liveness::DeviceLiveness;
use super::{GRANDMASTER_UNIT_ID, unit_id_of};

const IDLE_WAIT: Duration = Duration::from_secs(1);

pub(crate) struct Timeouts {
    pub(crate) ack: Duration,
    pub(crate) lost: Duration,
}

struct InflightRequest {
    msg_id: u16,
    sent_to: Option<DeviceSet>,
    sent_at: Instant,
    completion: ResponseSender,
    response: Response,
}

impl InflightRequest {
    fn has_all_replies(&self) -> bool {
        self.sent_to
            .is_some_and(|sent_to| sent_to.is_subset(&self.response.replied_devices()))
    }

    fn deliver(self) {
        tracing::trace!(msg_id = self.msg_id, "frame answered by every device");
        let _ = self.completion.send(Ok(self.response));
    }

    fn fail(self, error: Error) {
        let _ = self.completion.send(Err(error));
    }
}

struct InflightQueue {
    requests: VecDeque<Option<InflightRequest>>,
    head_ack_clock_started_at: Instant,
    slot_waiter: Option<Waker>,
}

impl InflightQueue {
    fn new(now: Instant) -> Self {
        Self {
            requests: VecDeque::with_capacity(MAX_INFLIGHT),
            head_ack_clock_started_at: now,
            slot_waiter: None,
        }
    }

    fn is_full(&self) -> bool {
        self.requests.len() == MAX_INFLIGHT
    }

    fn push(&mut self, request: InflightRequest) {
        if self.requests.is_empty() {
            self.head_ack_clock_started_at = request.sent_at;
        }
        self.requests.push_back(Some(request));
    }

    fn head(&self) -> Option<&InflightRequest> {
        self.requests.front()?.as_ref()
    }

    fn is_head(&self, msg_id: u16) -> bool {
        self.head().is_some_and(|head| head.msg_id == msg_id)
    }

    fn position(&self, msg_id: u16) -> Option<usize> {
        self.requests.iter().position(|request| {
            request
                .as_ref()
                .is_some_and(|request| request.msg_id == msg_id)
        })
    }

    fn take(&mut self, index: usize, now: Instant) -> Option<InflightRequest> {
        let request = self.requests[index].take();
        while matches!(self.requests.front(), Some(None)) {
            self.requests.pop_front();
            self.head_ack_clock_started_at = now;
        }
        while matches!(self.requests.back(), Some(None)) {
            self.requests.pop_back();
        }
        request
    }

    fn take_all(&mut self) -> Vec<InflightRequest> {
        self.requests.drain(..).flatten().collect()
    }

    fn head_ack_deadline(&self, ack_timeout: Duration) -> Option<Instant> {
        self.head()
            .and_then(|head| head.sent_to)
            .and(self.head_ack_clock_started_at.checked_add(ack_timeout))
    }
}

pub(crate) struct SessionState {
    inflight: InflightQueue,
    liveness: Vec<DeviceLiveness>,
    device_clock: DeviceClock,
    closed: Option<Error>,
    last_msg_id: u16,
    timeouts: Timeouts,
}

pub(crate) struct FailedRequests {
    requests: Vec<InflightRequest>,
    slot_waiter: Option<Waker>,
    error: Error,
}

impl SessionState {
    pub(crate) fn new(
        num_devices: usize,
        timeouts: Timeouts,
        device_clock: DeviceClock,
        now: Instant,
    ) -> Self {
        Self {
            inflight: InflightQueue::new(now),
            liveness: vec![DeviceLiveness::new(); num_devices],
            device_clock,
            closed: None,
            last_msg_id: 0,
            timeouts,
        }
    }

    pub(crate) fn closed_error(&self) -> Option<Error> {
        self.closed.clone()
    }

    fn next_msg_id(&mut self) -> u16 {
        self.last_msg_id = self.last_msg_id.wrapping_add(1);
        self.last_msg_id
    }

    pub(crate) fn poll_enqueue(
        &mut self,
        waker: &Waker,
        completion: &mut Option<ResponseSender>,
        now: Instant,
    ) -> Poll<Result<u16, Error>> {
        if let Some(error) = self.closed_error() {
            return Poll::Ready(Err(error));
        }
        if self.inflight.is_full() {
            self.inflight.slot_waiter = Some(waker.clone());
            return Poll::Pending;
        }
        let msg_id = self.next_msg_id();
        self.inflight.push(InflightRequest {
            msg_id,
            sent_to: None,
            sent_at: now,
            completion: completion
                .take()
                .expect("an enqueued frame is not polled again"),
            response: Response::for_devices(self.liveness.len()),
        });
        Poll::Ready(Ok(msg_id))
    }

    fn take_released_slot_waiter(&mut self) -> Option<Waker> {
        if self.inflight.is_full() {
            return None;
        }
        self.inflight.slot_waiter.take()
    }

    pub(crate) fn remove(&mut self, msg_id: u16, now: Instant) -> Option<ResponseSender> {
        let index = self.inflight.position(msg_id)?;
        self.inflight
            .take(index, now)
            .map(|request| request.completion)
    }

    fn record_sent_to(
        &mut self,
        msg_id: u16,
        sent_to: DeviceSet,
        now: Instant,
    ) -> Option<InflightRequest> {
        let index = self.inflight.position(msg_id)?;
        let request = self.inflight.requests[index].as_mut()?;
        request.sent_to = Some(sent_to);
        if request.has_all_replies() {
            return self.inflight.take(index, now);
        }
        if index == 0 {
            self.inflight.head_ack_clock_started_at = now;
        }
        None
    }

    fn record_reply(&mut self, reply: &Reply, now: Instant) -> Option<InflightRequest> {
        let Some(index) = self.inflight.position(reply.msg_id) else {
            tracing::trace!(msg_id = reply.msg_id, "dropping a reply nothing waits for");
            return None;
        };
        let request = self.inflight.requests[index].as_mut()?;
        if request.response.replied_devices()[reply.device] {
            tracing::trace!(
                msg_id = reply.msg_id,
                device = reply.device,
                "dropping a repeated reply"
            );
            return None;
        }
        request.response.record_reply(
            reply.device,
            reply.error_code,
            reply.sys_time,
            reply.data.as_slice(),
        );
        if !request.has_all_replies() {
            return None;
        }
        self.inflight.take(index, now)
    }

    fn devices_where(&self, holds: impl Fn(DeviceLiveness) -> bool) -> DeviceSet {
        self.liveness
            .iter()
            .enumerate()
            .filter(|&(_, &liveness)| holds(liveness))
            .map(|(device, _)| device)
            .collect()
    }

    fn device_status(&self) -> Vec<DeviceStatus> {
        self.liveness
            .iter()
            .map(|liveness| DeviceStatus {
                state: liveness.state(),
                ptp_locked: liveness.reported_flags().contains(Flags::PTP_LOCKED),
                failsafe_active: liveness.reported_flags().contains(Flags::FAILSAFE),
            })
            .collect()
    }

    fn start_unanswered_clock(&mut self, device: usize, requested_at: Instant) -> bool {
        self.liveness[device].start_unanswered_clock(requested_at)
    }

    fn record_send_failure(&mut self, device: usize, now: Instant) {
        self.liveness[device].record_send_failure(now, self.timeouts.lost);
    }

    fn receive_reply(&mut self, reply: &DeviceReply, now: Instant) -> Option<InflightRequest> {
        let device = reply.reply.device;
        let expected_unit_id = unit_id_of(device);
        let liveness = &mut self.liveness[device];
        let accepted =
            liveness.accept_reply(expected_unit_id, reply.reported_unit_id, reply.flags, now);
        if !accepted {
            return None;
        }
        if expected_unit_id == GRANDMASTER_UNIT_ID
            && let Some(sys_time) = NonZeroU64::new(reply.reply.sys_time)
        {
            self.device_clock
                .observe(SysTime::from_nanos(sys_time.get()), now);
        }
        self.record_reply(&reply.reply, now)
    }

    pub(crate) fn mark_unanswered_devices_lost(&mut self, now: Instant) {
        for liveness in &mut self.liveness {
            liveness.mark_lost_if_unanswered(now, self.timeouts.lost);
        }
    }

    pub(crate) fn lost_deadline(&self) -> Option<Instant> {
        self.liveness
            .iter()
            .filter_map(|liveness| liveness.lost_deadline(self.timeouts.lost))
            .min()
    }

    pub(crate) fn is_head_ack_overdue(&self, now: Instant) -> bool {
        self.inflight
            .head_ack_deadline(self.timeouts.ack)
            .is_some_and(|deadline| now >= deadline)
    }

    pub(crate) fn next_deadline(&self, now: Instant) -> Instant {
        [
            self.lost_deadline(),
            self.inflight.head_ack_deadline(self.timeouts.ack),
        ]
        .into_iter()
        .flatten()
        .fold(now + IDLE_WAIT, Instant::min)
    }

    fn fail_all(&mut self, error: Error) -> FailedRequests {
        FailedRequests {
            requests: self.inflight.take_all(),
            slot_waiter: self.inflight.slot_waiter.take(),
            error,
        }
    }

    pub(crate) fn close(&mut self, error: Error) -> FailedRequests {
        self.closed = Some(error.clone());
        self.fail_all(error)
    }
}

impl FailedRequests {
    fn resolve(self) {
        for request in self.requests {
            request.fail(self.error.clone());
        }
        if let Some(slot_waiter) = self.slot_waiter {
            slot_waiter.wake();
        }
    }
}

struct EachDeviceSend {
    accepted_devices: DeviceSet,
    first_failure: Option<UdpError>,
    started_lost_clock: bool,
}

pub(crate) struct Session {
    channel: Channel,
    state: Mutex<SessionState>,
    stop_requested: AtomicBool,
    heartbeat_resume_device: AtomicUsize,
}

impl Session {
    pub(crate) fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, UdpError> {
        let mut device_clock = DeviceClock::default();
        let channel = super::open::open(option, geometry.num_devices(), &mut device_clock)?;
        let num_devices = channel.num_devices();
        Ok(Self {
            state: Mutex::new(SessionState::new(
                num_devices,
                Timeouts {
                    ack: option.ack_timeout,
                    lost: option.lost_timeout,
                },
                device_clock,
                Instant::now(),
            )),
            stop_requested: AtomicBool::new(false),
            heartbeat_resume_device: AtomicUsize::new(num_devices - 1),
            channel,
        })
    }

    fn lock_state(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn release(mut state: MutexGuard<'_, SessionState>) {
        let slot_waiter = state.take_released_slot_waiter();
        drop(state);
        if let Some(slot_waiter) = slot_waiter {
            slot_waiter.wake();
        }
    }

    pub(crate) fn socket(&self) -> &std::net::UdpSocket {
        self.channel.socket()
    }

    pub(crate) fn device_status(&self) -> Result<Vec<DeviceStatus>, Error> {
        let state = self.lock_state();
        if let Some(error) = self.closed_error_of(&state) {
            return Err(error);
        }
        Ok(state.device_status())
    }

    pub(crate) fn device_time(&self) -> Option<SysTime> {
        self.lock_state().device_clock.now(Instant::now())
    }

    pub(crate) fn ack_timeout(&self) -> Duration {
        self.lock_state().timeouts.ack
    }

    pub(crate) fn lost_timeout(&self) -> Duration {
        self.lock_state().timeouts.lost
    }

    pub(crate) fn set_ack_timeout(&self, timeout: Duration) {
        self.lock_state().timeouts.ack = timeout;
        self.wake_driver("the new ack timeout is noticed at its next deadline");
    }

    pub(crate) fn set_lost_timeout(&self, timeout: Duration) {
        self.lock_state().timeouts.lost = timeout;
        self.wake_driver("the new lost timeout is noticed at its next deadline");
    }

    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }

    pub(crate) fn closed_error(&self) -> Option<Error> {
        self.closed_error_of(&self.lock_state())
    }

    fn closed_error_of(&self, state: &SessionState) -> Option<Error> {
        state
            .closed_error()
            .or_else(|| self.is_stopping().then_some(Error::Closed))
    }

    pub(crate) fn wake(&self) -> bool {
        match self.channel.wake() {
            Ok(()) => true,
            Err(e) => {
                tracing::debug!("waking the receiver failed: {e}");
                false
            }
        }
    }

    pub(crate) fn wait_readable(&self, deadline: Instant) -> Result<(), UdpError> {
        Ok(self.channel.wait_readable(deadline)?)
    }

    pub(crate) fn poll_enqueue(
        &self,
        cx: &mut Context<'_>,
        completion: &mut Option<ResponseSender>,
    ) -> Poll<Result<u16, Error>> {
        self.lock_state()
            .poll_enqueue(cx.waker(), completion, Instant::now())
    }

    pub(crate) fn cancel(&self, msg_id: u16) {
        let mut state = self.lock_state();
        state.remove(msg_id, Instant::now());
        Self::release(state);
    }

    fn send_to_each_device<'a>(
        &self,
        kind: Kind,
        msg_id: u16,
        target_devices: DeviceSet,
        start_device: usize,
        body_for_device: impl Fn(usize) -> [&'a [u8]; 2],
    ) -> EachDeviceSend {
        let mut retry_full_socket_until = match kind {
            Kind::Frame => Instant::now().checked_add(self.lost_timeout()),
            _ => None,
        };
        let mut accepted_devices = DeviceSet::empty();
        let mut first_failure = None;
        let mut started_lost_clock = false;
        let num_devices = self.channel.num_devices();
        for device in (0..num_devices).map(|step| (start_device + num_devices - step) % num_devices)
        {
            if !target_devices[device] {
                continue;
            }
            let result = self.channel.send_to_device(
                device,
                kind,
                msg_id,
                body_for_device(device),
                retry_full_socket_until,
            );
            match result {
                Ok(attempted_at) => {
                    started_lost_clock |= self
                        .lock_state()
                        .start_unanswered_clock(device, attempted_at);
                    accepted_devices.insert(device);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if kind == Kind::Frame {
                        retry_full_socket_until = None;
                        tracing::warn!(
                            device,
                            "the socket stayed full past the lost timeout; skipping the device"
                        );
                        first_failure.get_or_insert(UdpError::Unsent { device, source: e });
                    } else {
                        tracing::trace!(
                            device,
                            "the socket is full; the next heartbeat resumes from this device"
                        );
                        self.heartbeat_resume_device
                            .store(device, Ordering::Relaxed);
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(device, "sending to a device failed; skipping it: {e}");
                    self.lock_state()
                        .record_send_failure(device, Instant::now());
                    first_failure.get_or_insert(UdpError::Unsent { device, source: e });
                }
            }
        }
        EachDeviceSend {
            accepted_devices,
            first_failure,
            started_lost_clock,
        }
    }

    fn wake_driver(&self, missed_wake: &str) {
        if !self.wake() {
            tracing::debug!("could not wake the driver; {missed_wake}");
        }
    }

    pub(crate) fn send_frame(
        &self,
        msg_id: u16,
        frame: Frame<'_>,
        exec_at: u32,
    ) -> Result<(), Error> {
        let datagrams = frame.datagrams();
        let target_devices = self
            .lock_state()
            .devices_where(|liveness| !liveness.is_lost());
        let mut headers = [FrameHeader::new_zeroed(); MAX_DEVICES];
        for (header, datagram) in headers.iter_mut().zip(datagrams) {
            *header = datagram.frame_header(exec_at);
        }
        let outcome = self.send_to_each_device(
            Kind::Frame,
            msg_id,
            target_devices,
            self.channel.num_devices() - 1,
            |device| {
                let index = frame.datagram_index(device);
                [headers[index].as_bytes(), datagrams[index].payload()]
            },
        );
        let sent = outcome
            .first_failure
            .map_or(Ok(outcome.accepted_devices), Err);
        let finished = self.finish_frame_send(msg_id, sent);
        if finished.as_ref().is_ok_and(|&became_head| became_head) {
            self.wake_driver("the ack timeout is noticed at its next deadline");
        } else if outcome.started_lost_clock {
            self.wake_driver("a silent device is noticed at its next deadline");
        }
        finished.map(|_| ())
    }

    fn finish_frame_send(
        &self,
        msg_id: u16,
        sent: Result<DeviceSet, UdpError>,
    ) -> Result<bool, Error> {
        let now = Instant::now();
        let mut state = self.lock_state();
        let sent_to = match sent {
            Ok(sent_to) => sent_to,
            Err(e) => {
                state.remove(msg_id, now);
                Self::release(state);
                return Err(e.into());
            }
        };
        if sent_to.is_empty() {
            let completion = state.remove(msg_id, now);
            let timeout = state.timeouts.ack;
            Self::release(state);
            if let Some(completion) = completion {
                let _ = completion.send(Err(Error::Timeout { timeout }));
            }
            return Ok(false);
        }
        let answered = state.record_sent_to(msg_id, sent_to, now);
        let became_head = state.inflight.is_head(msg_id);
        Self::release(state);
        if let Some(answered) = answered {
            answered.deliver();
        }
        Ok(became_head)
    }

    pub(crate) fn send_heartbeat(&self) -> Result<(), Error> {
        let (msg_id, target_devices) = {
            let mut state = self.lock_state();
            if let Some(error) = self.closed_error_of(&state) {
                return Err(error);
            }
            (
                state.next_msg_id(),
                state.devices_where(|liveness| !liveness.is_lost()),
            )
        };
        let outcome = self.send_to_each_device(
            Kind::Heartbeat,
            msg_id,
            target_devices,
            self.heartbeat_resume_device.load(Ordering::Relaxed),
            |_| [&[][..]; 2],
        );
        if outcome.started_lost_clock {
            self.wake_driver("a silent device is noticed at its next deadline");
        }
        Ok(())
    }

    fn receive_available(&self) -> Result<(), NetworkCause> {
        loop {
            let polled = self.channel.try_recv_from_device().map_err(|e| {
                tracing::error!("receiving failed: {e}");
                NetworkCause::new(UdpError::from(e))
            })?;
            match polled {
                Polled::Empty => {
                    self.lock_state()
                        .mark_unanswered_devices_lost(Instant::now());
                    return Ok(());
                }
                Polled::Wake => return Ok(()),
                Polled::Device(reply) => self.handle_reply(&reply),
            }
        }
    }

    fn handle_reply(&self, reply: &DeviceReply) {
        let mut state = self.lock_state();
        let answered = state.receive_reply(reply, Instant::now());
        Self::release(state);
        if let Some(answered) = answered {
            answered.deliver();
        }
    }

    pub(crate) fn step(&self) -> Result<Instant, NetworkCause> {
        self.receive_available()?;
        let now = Instant::now();
        if self.lock_state().is_head_ack_overdue(now) {
            self.receive_available()?;
        }
        self.fail_all_if_head_ack_overdue(now);
        Ok(self.lock_state().next_deadline(now))
    }

    fn fail_all_if_head_ack_overdue(&self, now: Instant) {
        let mut state = self.lock_state();
        if !state.is_head_ack_overdue(now) {
            return;
        }
        let timeout = state.timeouts.ack;
        let failed = state.fail_all(Error::Timeout { timeout });
        drop(state);
        tracing::warn!(
            pending = failed.requests.len(),
            "the head frame got no reply within the ack timeout; failing the pending frames"
        );
        failed.resolve();
    }

    pub(crate) fn close(&self, error: Error) {
        let failed = self.lock_state().close(error);
        failed.resolve();
    }
}

#[cfg(test)]
mod tests {
    use autd3_cpu_wire::udp::Flags;
    use autd3_rs_core::DeviceState;

    use super::*;

    const TIMEOUTS: Timeouts = Timeouts {
        ack: Duration::from_millis(10),
        lost: Duration::from_millis(100),
    };
    const READY: Flags = Flags::ASSIGNED
        .union(Flags::PTP_LOCKED)
        .union(Flags::SYNC_READY);

    fn session_state(num_devices: usize, now: Instant) -> SessionState {
        SessionState::new(num_devices, TIMEOUTS, DeviceClock::default(), now)
    }

    fn enqueue(state: &mut SessionState, now: Instant) -> (u16, crate::client::ResponseFuture) {
        let (completion, response) = super::super::completion::channel();
        let mut completion = Some(completion);
        let Poll::Ready(Ok(msg_id)) = state.poll_enqueue(Waker::noop(), &mut completion, now)
        else {
            panic!("the queue has room");
        };
        (msg_id, response)
    }

    fn reply(device: usize, msg_id: u16, error_code: u8) -> Reply {
        Reply::new(device, msg_id, error_code, 0, &[])
    }

    #[test]
    fn a_full_queue_parks_the_sender_until_the_head_is_released() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued: Vec<_> = (0..MAX_INFLIGHT)
            .map(|_| enqueue(&mut state, now))
            .collect();
        let (completion, _response) = super::super::completion::channel();
        let mut completion = Some(completion);

        assert!(
            state
                .poll_enqueue(Waker::noop(), &mut completion, now)
                .is_pending()
        );
        assert!(state.take_released_slot_waiter().is_none());

        let head = enqueued[0].0;
        assert!(
            state
                .record_sent_to(head, DeviceSet::from_iter([0]), now)
                .is_none()
        );
        assert!(state.record_reply(&reply(0, head, 0), now).is_some());

        assert!(state.take_released_slot_waiter().is_some());
        assert!(matches!(
            state.poll_enqueue(Waker::noop(), &mut completion, now),
            Poll::Ready(Ok(_))
        ));
    }

    #[test]
    fn a_frame_behind_the_head_keeps_its_slot_until_the_head_is_done() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued: Vec<_> = (0..MAX_INFLIGHT)
            .map(|_| enqueue(&mut state, now))
            .collect();
        let behind_head = enqueued[1].0;
        assert!(state.remove(behind_head, now).is_some());
        assert!(state.inflight.is_full());
        assert!(state.take_released_slot_waiter().is_none());

        let head = enqueued[0].0;
        assert!(state.remove(head, now).is_some());
        assert_eq!(state.inflight.requests.len(), MAX_INFLIGHT - 2);
        assert!(state.inflight.is_head(enqueued[2].0));
    }

    #[test]
    fn the_queue_keeps_the_order_over_many_rounds() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        for _ in 0..MAX_INFLIGHT * 2 + 3 {
            let (msg_id, _response) = enqueue(&mut state, now);
            assert!(state.inflight.is_head(msg_id));
            assert!(state.remove(msg_id, now).is_some());
            assert_eq!(state.inflight.requests.len(), 0);
        }
        let first = enqueue(&mut state, now).0;
        let second = enqueue(&mut state, now).0;
        assert!(state.inflight.is_head(first));
        assert!(state.remove(first, now).is_some());
        assert!(state.inflight.is_head(second));
    }

    fn fill_and_park(state: &mut SessionState, now: Instant) -> Vec<u16> {
        let enqueued = (0..MAX_INFLIGHT).map(|_| enqueue(state, now).0).collect();
        let (completion, _response) = super::super::completion::channel();
        let mut completion = Some(completion);
        assert!(
            state
                .poll_enqueue(Waker::noop(), &mut completion, now)
                .is_pending()
        );
        enqueued
    }

    #[test]
    fn cancelling_the_newest_frame_frees_its_slot_at_once() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued = fill_and_park(&mut state, now);

        assert!(state.remove(enqueued[MAX_INFLIGHT - 1], now).is_some());

        assert_eq!(state.inflight.requests.len(), MAX_INFLIGHT - 1);
        assert!(state.take_released_slot_waiter().is_some());
        let (msg_id, _response) = enqueue(&mut state, now);
        assert_eq!(state.inflight.position(msg_id), Some(MAX_INFLIGHT - 1));
    }

    #[test]
    fn a_hole_left_behind_the_head_is_freed_once_the_frames_after_it_are_gone() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued = fill_and_park(&mut state, now);
        assert!(state.remove(enqueued[MAX_INFLIGHT - 2], now).is_some());
        assert!(state.take_released_slot_waiter().is_none());

        assert!(state.remove(enqueued[MAX_INFLIGHT - 1], now).is_some());

        assert_eq!(state.inflight.requests.len(), MAX_INFLIGHT - 2);
        assert!(state.take_released_slot_waiter().is_some());
    }

    #[test]
    fn failing_every_pending_frame_releases_the_parked_sender() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        fill_and_park(&mut state, now);

        assert_eq!(state.inflight.take_all().len(), MAX_INFLIGHT);

        assert!(state.take_released_slot_waiter().is_some());
    }

    #[test]
    fn closing_on_a_network_failure_resolves_with_that_failure() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (_msg_id, response) = enqueue(&mut state, now);
        let cause = NetworkCause::new(UdpError::NoInterfaceFound);

        state.close(Error::Network(cause)).resolve();

        assert!(matches!(
            pollster::block_on(response),
            Err(Error::Network(_))
        ));
        assert!(matches!(state.closed_error(), Some(Error::Network(_))));
    }

    #[test]
    fn closing_resolves_the_pending_frames_and_wakes_the_parked_sender() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let mut enqueued: Vec<_> = (0..MAX_INFLIGHT)
            .map(|_| enqueue(&mut state, now))
            .collect();
        let (completion, _response) = super::super::completion::channel();
        let mut completion = Some(completion);
        assert!(
            state
                .poll_enqueue(Waker::noop(), &mut completion, now)
                .is_pending()
        );

        let failed = state.close(Error::Closed);
        assert!(failed.slot_waiter.is_some());
        failed.resolve();

        assert!(matches!(
            pollster::block_on(enqueued.remove(0).1),
            Err(Error::Closed)
        ));
        assert!(matches!(
            state.poll_enqueue(Waker::noop(), &mut completion, now),
            Poll::Ready(Err(Error::Closed))
        ));
    }

    #[test]
    fn a_frame_answered_before_its_send_returns_is_handed_back_when_the_send_does() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());
        assert!(state.record_reply(&reply(1, msg_id, 0), now).is_none());

        let answered = state.record_sent_to(msg_id, DeviceSet::from_iter([0, 1]), now);

        assert_eq!(answered.map(|answered| answered.msg_id), Some(msg_id));
        assert_eq!(state.inflight.requests.len(), 0);
    }

    #[test]
    fn a_frame_still_missing_a_reply_stays_pending_after_its_send_returns() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());

        assert!(
            state
                .record_sent_to(msg_id, DeviceSet::from_iter([0, 1]), now)
                .is_none()
        );

        assert_eq!(state.inflight.requests.len(), 1);
        assert_eq!(
            state.inflight.head().unwrap().sent_to,
            Some(DeviceSet::from_iter([0, 1]))
        );
    }

    #[test]
    fn a_frame_is_never_answered_while_its_send_is_still_running() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());
        assert_eq!(state.inflight.requests.len(), 1);
    }

    #[test]
    fn only_the_frame_at_the_head_asks_for_a_wake() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (head, _head_response) = enqueue(&mut state, now);
        let (next, _next_response) = enqueue(&mut state, now);
        assert!(
            state
                .record_sent_to(head, DeviceSet::from_iter([0]), now)
                .is_none()
        );
        assert!(
            state
                .record_sent_to(next, DeviceSet::from_iter([0]), now)
                .is_none()
        );

        assert!(state.inflight.is_head(head));
        assert!(!state.inflight.is_head(next));
    }

    #[test]
    fn a_repeated_reply_from_a_device_keeps_the_first_one() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, response) = enqueue(&mut state, now);
        assert!(
            state
                .record_sent_to(msg_id, DeviceSet::from_iter([0, 1]), now)
                .is_none()
        );

        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());
        assert!(state.record_reply(&reply(0, msg_id, 5), now).is_none());
        let answered = state.record_reply(&reply(1, msg_id, 0), now).unwrap();
        answered.deliver();

        assert_eq!(pollster::block_on(response).unwrap().error_codes(), [0, 0]);
    }

    #[test]
    fn a_reply_nothing_waits_for_is_dropped() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(
            state
                .record_reply(&reply(0, msg_id.wrapping_add(1), 0), now)
                .is_none()
        );
        assert_eq!(state.inflight.requests.len(), 1);
    }

    #[test]
    fn the_ack_clock_of_the_head_starts_when_its_send_returns_or_it_becomes_the_head() {
        let start = Instant::now();
        let mut state = session_state(1, start);
        let (head, _head_response) = enqueue(&mut state, start);
        let (next, _next_response) = enqueue(&mut state, start);
        assert_eq!(state.inflight.head_ack_deadline(TIMEOUTS.ack), None);

        let head_sent = start + Duration::from_millis(1);
        assert!(
            state
                .record_sent_to(head, DeviceSet::from_iter([0]), head_sent)
                .is_none()
        );
        let next_sent = start + Duration::from_millis(2);
        assert!(
            state
                .record_sent_to(next, DeviceSet::from_iter([0]), next_sent)
                .is_none()
        );
        assert_eq!(
            state.inflight.head_ack_deadline(TIMEOUTS.ack),
            Some(head_sent + TIMEOUTS.ack)
        );

        let head_answered = start + Duration::from_millis(5);
        assert!(
            state
                .record_reply(&reply(0, head, 0), head_answered)
                .is_some()
        );
        assert!(state.inflight.is_head(next));
        assert_eq!(
            state.inflight.head_ack_deadline(TIMEOUTS.ack),
            Some(head_answered + TIMEOUTS.ack)
        );
        assert!(!state.is_head_ack_overdue(head_answered + TIMEOUTS.ack / 2));
        assert!(state.is_head_ack_overdue(head_answered + TIMEOUTS.ack));
    }

    #[test]
    fn the_next_deadline_is_the_earliest_of_the_ack_the_lost_and_the_idle_wait() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        assert_eq!(state.next_deadline(now), now + IDLE_WAIT);

        state.start_unanswered_clock(0, now);
        assert_eq!(state.next_deadline(now), now + TIMEOUTS.lost);

        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(
            state
                .record_sent_to(msg_id, DeviceSet::from_iter([0]), now)
                .is_none()
        );
        assert_eq!(state.next_deadline(now), now + TIMEOUTS.ack);
    }

    #[test]
    fn a_changed_ack_timeout_moves_the_deadline_of_the_pending_head_from_its_clock_start() {
        let start = Instant::now();
        let mut state = session_state(1, start);
        let (head, _response) = enqueue(&mut state, start);
        let head_sent = start + Duration::from_millis(1);
        assert!(
            state
                .record_sent_to(head, DeviceSet::from_iter([0]), head_sent)
                .is_none()
        );
        let later = head_sent + TIMEOUTS.ack;
        assert!(state.is_head_ack_overdue(later));

        state.timeouts.ack = TIMEOUTS.ack * 3;

        assert!(!state.is_head_ack_overdue(later));
        assert_eq!(state.next_deadline(later), head_sent + TIMEOUTS.ack * 3);

        state.timeouts.ack = TIMEOUTS.ack / 2;

        assert!(state.is_head_ack_overdue(head_sent + TIMEOUTS.ack / 2));
    }

    #[test]
    fn a_changed_lost_timeout_moves_the_deadline_of_an_unanswered_device_from_its_first_request() {
        let start = Instant::now();
        let mut state = session_state(2, start);
        state.start_unanswered_clock(1, start);
        assert_eq!(state.lost_deadline(), Some(start + TIMEOUTS.lost));

        state.timeouts.lost = TIMEOUTS.lost * 2;
        state.mark_unanswered_devices_lost(start + TIMEOUTS.lost);

        assert!(!state.liveness[1].is_lost());
        assert_eq!(state.lost_deadline(), Some(start + TIMEOUTS.lost * 2));

        state.timeouts.lost = TIMEOUTS.lost / 2;
        state.mark_unanswered_devices_lost(start + TIMEOUTS.lost / 2);

        assert!(state.liveness[1].is_lost());
        assert!(!state.liveness[0].is_lost());
    }

    #[test]
    fn a_lost_device_stays_lost_and_takes_no_frame() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        state.start_unanswered_clock(1, now);
        state.mark_unanswered_devices_lost(now + TIMEOUTS.lost);

        let answered = state.receive_reply(
            &DeviceReply {
                reply: reply(1, 0, 0),
                reported_unit_id: 1,
                flags: READY,
            },
            now + TIMEOUTS.lost,
        );
        assert!(answered.is_none());
        assert_eq!(state.liveness[1].state(), DeviceState::Lost);
        assert_eq!(
            state.devices_where(|liveness| !liveness.is_lost()),
            DeviceSet::from_iter([0])
        );
    }

    fn timed_reply(device: usize, reported_unit_id: u8, sys_time: u64) -> DeviceReply {
        DeviceReply {
            reply: Reply::new(device, 0, 0, sys_time, &[]),
            reported_unit_id,
            flags: READY,
        }
    }

    const DEVICE_TIME_NS: u64 = 1 << 50;

    #[test]
    fn only_an_accepted_timed_reply_of_device_0_observes_the_device_clock() {
        let now = Instant::now();
        let mut state = session_state(2, now);

        state.receive_reply(&timed_reply(1, 1, DEVICE_TIME_NS), now);
        assert!(state.device_clock.now(now).is_none());
        state.receive_reply(&timed_reply(0, 0, 0), now);
        assert!(state.device_clock.now(now).is_none());
        state.receive_reply(&timed_reply(0, 0, DEVICE_TIME_NS), now);
        assert!(state.device_clock.now(now).is_some());
    }

    #[test]
    fn a_reply_under_a_foreign_id_observes_nothing() {
        let now = Instant::now();
        let mut state = session_state(1, now);

        assert!(
            state
                .receive_reply(&timed_reply(0, 1, DEVICE_TIME_NS), now)
                .is_none()
        );
        assert!(state.liveness[0].is_lost());
        assert!(state.device_clock.now(now).is_none());
    }

    fn flagged_reply(device: usize, reported_unit_id: u8, flags: Flags) -> DeviceReply {
        DeviceReply {
            reply: reply(device, 0, 0),
            reported_unit_id,
            flags,
        }
    }

    #[test]
    fn the_device_status_reports_each_device_liveness() {
        let now = Instant::now();
        let mut state = session_state(3, now);
        state.receive_reply(&flagged_reply(1, 1, Flags::ASSIGNED), now);
        state.receive_reply(&flagged_reply(2, 0, READY), now);

        let status = state.device_status();

        assert_eq!(
            status.iter().map(|s| s.state).collect::<Vec<_>>(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Lost]
        );
        assert_eq!(
            status.iter().map(|s| s.ptp_locked).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert!(status.iter().all(|s| !s.failsafe_active));
    }

    #[test]
    fn the_device_status_reports_the_flags_of_the_last_reply() {
        let now = Instant::now();
        let mut state = session_state(3, now);
        state.receive_reply(
            &flagged_reply(0, 0, Flags::ASSIGNED | Flags::PTP_LOCKED),
            now,
        );
        state.receive_reply(
            &flagged_reply(1, 1, Flags::ASSIGNED | Flags::FAILSAFE | Flags::SYNC_READY),
            now,
        );
        state.receive_reply(&flagged_reply(2, 2, READY | Flags::FAILSAFE), now);

        let status = state.device_status();

        assert_eq!(
            status,
            [
                DeviceStatus {
                    state: DeviceState::Syncing,
                    ptp_locked: true,
                    failsafe_active: false,
                },
                DeviceStatus {
                    state: DeviceState::Syncing,
                    ptp_locked: false,
                    failsafe_active: true,
                },
                DeviceStatus {
                    state: DeviceState::Ready,
                    ptp_locked: true,
                    failsafe_active: true,
                },
            ]
        );
    }

    #[test]
    fn a_closed_session_keeps_the_device_liveness() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        state.close(Error::Closed).resolve();
        assert!(matches!(state.closed_error(), Some(Error::Closed)));
        assert_eq!(state.liveness[0].state(), DeviceState::Ready);
    }
}

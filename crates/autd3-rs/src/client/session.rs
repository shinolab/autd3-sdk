use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Flags, Kind};
use autd3_rs_core::MAX_DEVICES;
use tokio::sync::Notify;
use zerocopy::{FromZeros, IntoBytes};

use crate::datagram::Frame;
use crate::device_set::DeviceSet;
use crate::device_status::DeviceStatus;
use crate::error::{Error, NetworkCause};
use crate::geometry::Geometry;
use crate::protocol::{FrameHeader, MAX_INFLIGHT};
use crate::response::Response;
use crate::udp::{Channel, DeviceReply, Reply, TransportOption, UdpError};
use crate::value::SysTime;

use super::GRANDMASTER_UNIT_ID;
use super::completion::{self, ResponseFuture, ResponseSender};
use super::device_clock::DeviceClock;
use super::liveness::DeviceLiveness;

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
}

impl InflightQueue {
    fn new(now: Instant) -> Self {
        Self {
            requests: VecDeque::with_capacity(MAX_INFLIGHT),
            head_ack_clock_started_at: now,
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
    pub(crate) device_clock: DeviceClock,
    closed: Option<Error>,
    last_msg_id: u16,
    pub(crate) timeouts: Timeouts,
}

pub(crate) struct FailedRequests {
    requests: Vec<InflightRequest>,
    error: Error,
}

impl SessionState {
    pub(crate) fn new(
        num_devices: usize,
        timeouts: Timeouts,
        device_clock: DeviceClock,
        initial_liveness: DeviceLiveness,
        now: Instant,
    ) -> Self {
        Self {
            inflight: InflightQueue::new(now),
            liveness: vec![initial_liveness; num_devices],
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

    fn try_enqueue(&mut self, now: Instant) -> Result<Option<(u16, ResponseFuture)>, Error> {
        if let Some(error) = self.closed_error() {
            return Err(error);
        }
        if self.inflight.is_full() {
            return Ok(None);
        }
        let msg_id = self.next_msg_id();
        let (completion, response) = completion::channel();
        self.inflight.push(InflightRequest {
            msg_id,
            sent_to: None,
            sent_at: now,
            completion,
            response: Response::for_devices(self.liveness.len()),
        });
        Ok(Some((msg_id, response)))
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

    fn record_reply(&mut self, reply: Reply, now: Instant) -> Option<InflightRequest> {
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
        request
            .response
            .record_reply(reply.device, reply.error_code, reply.sys_time, reply.data);
        if !request.has_all_replies() {
            return None;
        }
        self.inflight.take(index, now)
    }

    pub(crate) fn device_status(&self) -> Vec<DeviceStatus> {
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

    fn record_send_failure(&mut self, device: usize, now: Instant) -> bool {
        self.liveness[device].record_send_failure(now, self.timeouts.lost)
    }

    fn receive_reply(
        &mut self,
        reply: DeviceReply,
        now: Instant,
    ) -> Result<Option<InflightRequest>, Error> {
        let device = reply.reply.device;
        let liveness = &mut self.liveness[device];
        let accepted = liveness.accept_reply(
            device,
            usize::from(reply.reported_unit_id),
            reply.flags,
            now,
        );
        if !accepted {
            return Err(Error::DeviceLost { device });
        }
        if device == GRANDMASTER_UNIT_ID
            && let Some(sys_time) = NonZeroU64::new(reply.reply.sys_time)
        {
            self.device_clock
                .observe(SysTime::from_nanos(sys_time.get()), now);
        }
        Ok(self.record_reply(reply.reply, now))
    }

    pub(crate) fn first_device_unanswered_past_lost_timeout(&self, now: Instant) -> Option<usize> {
        self.liveness
            .iter()
            .position(|liveness| liveness.is_unanswered_past(now, self.timeouts.lost))
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
            error,
        }
    }

    pub(crate) fn close(&mut self, error: Error) -> FailedRequests {
        let error = self.closed.get_or_insert(error).clone();
        self.fail_all(error)
    }
}

impl FailedRequests {
    pub(crate) fn resolve(self) {
        for request in self.requests {
            request.fail(self.error.clone());
        }
    }
}

struct EachDeviceSend {
    accepted_devices: DeviceSet,
    first_failure: Option<UdpError>,
    started_lost_clock: bool,
}

pub(crate) struct EnqueuedFrame<'a> {
    session: &'a Session,
    msg_id: u16,
}

impl EnqueuedFrame<'_> {
    pub(crate) fn msg_id(&self) -> u16 {
        self.msg_id
    }
}

impl Drop for EnqueuedFrame<'_> {
    fn drop(&mut self) {
        let mut state = self.session.lock_state();
        state.remove(self.msg_id, Instant::now());
        self.session.release(state);
    }
}

pub(crate) struct Session {
    channel: Channel,
    state: Mutex<SessionState>,
    stop_requested: AtomicBool,
    heartbeat_resume_device: AtomicUsize,
    io_wake: Notify,
    slot_released: Notify,
}

impl Session {
    pub(crate) async fn open(
        geometry: &Geometry,
        option: &TransportOption,
    ) -> Result<Self, UdpError> {
        let mut device_clock = DeviceClock::default();
        let channel = super::open::open(option, geometry.num_devices(), &mut device_clock).await?;
        let timeouts = Timeouts {
            ack: option.ack_timeout,
            lost: option.lost_timeout,
        };
        let initial_liveness = if option.sync_on_open {
            DeviceLiveness::new()
        } else {
            DeviceLiveness::unsynchronized()
        };
        Ok(Self::new(channel, timeouts, device_clock, initial_liveness))
    }

    fn new(
        channel: Channel,
        timeouts: Timeouts,
        device_clock: DeviceClock,
        initial_liveness: DeviceLiveness,
    ) -> Self {
        let num_devices = channel.num_devices();
        Self {
            state: Mutex::new(SessionState::new(
                num_devices,
                timeouts,
                device_clock,
                initial_liveness,
                Instant::now(),
            )),
            stop_requested: AtomicBool::new(false),
            heartbeat_resume_device: AtomicUsize::new(num_devices - 1),
            io_wake: Notify::new(),
            slot_released: Notify::new(),
            channel,
        }
    }

    pub(crate) fn lock_state(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn release(&self, state: MutexGuard<'_, SessionState>) {
        let has_room = !state.inflight.is_full();
        drop(state);
        if has_room {
            self.slot_released.notify_one();
        }
    }

    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
        self.wake_io();
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }

    pub(crate) fn closed_error_of(&self, state: &SessionState) -> Option<Error> {
        state
            .closed_error()
            .or_else(|| self.is_stopping().then_some(Error::Closed))
    }

    pub(crate) fn wake_io(&self) {
        self.io_wake.notify_one();
    }

    pub(crate) async fn enqueue(&self) -> Result<(EnqueuedFrame<'_>, ResponseFuture), Error> {
        loop {
            if let Some((msg_id, response)) = self.lock_state().try_enqueue(Instant::now())? {
                let session = self;
                return Ok((EnqueuedFrame { session, msg_id }, response));
            }
            self.slot_released.notified().await;
        }
    }

    async fn send_to_each_device<'a>(
        &self,
        kind: Kind,
        msg_id: u16,
        start_device: usize,
        body_for_device: impl Fn(usize) -> [&'a [u8]; 2] + Send,
    ) -> EachDeviceSend {
        let mut retry_full_socket_until = if kind == Kind::Frame {
            Instant::now().checked_add(self.lock_state().timeouts.lost)
        } else {
            None
        };
        let mut accepted_devices = DeviceSet::empty();
        let mut first_failure = None;
        let mut started_lost_clock = false;
        let mut lost_device = None;
        let num_devices = self.channel.num_devices();
        for device in (0..num_devices).map(|step| (start_device + num_devices - step) % num_devices)
        {
            let result = self
                .channel
                .send_to_device(
                    device,
                    kind,
                    msg_id,
                    body_for_device(device),
                    retry_full_socket_until,
                )
                .await;
            match result {
                Ok(attempted_at) => {
                    accepted_devices.insert(device);
                    started_lost_clock |= self
                        .lock_state()
                        .start_unanswered_clock(device, attempted_at);
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
                    if self
                        .lock_state()
                        .record_send_failure(device, Instant::now())
                    {
                        lost_device.get_or_insert(device);
                    }
                    first_failure.get_or_insert(UdpError::Unsent { device, source: e });
                }
            }
        }
        if let Some(device) = lost_device {
            self.close(Error::DeviceLost { device });
        }
        EachDeviceSend {
            accepted_devices,
            first_failure,
            started_lost_clock,
        }
    }

    pub(crate) async fn send_frame(
        &self,
        enqueued: EnqueuedFrame<'_>,
        frame: Frame<'_>,
        exec_at: u32,
    ) -> Result<(), Error> {
        let datagrams = frame.datagrams();
        let mut headers = [FrameHeader::new_zeroed(); MAX_DEVICES];
        for (header, datagram) in headers.iter_mut().zip(datagrams) {
            *header = datagram.frame_header(exec_at);
        }
        let msg_id = enqueued.msg_id;
        let outcome = self
            .send_to_each_device(
                Kind::Frame,
                msg_id,
                self.channel.num_devices() - 1,
                |device| {
                    let index = frame.datagram_index(device);
                    [headers[index].as_bytes(), datagrams[index].payload()]
                },
            )
            .await;
        std::mem::forget(enqueued);
        let sent = outcome
            .first_failure
            .map_or(Ok(outcome.accepted_devices), Err);
        let finished = self.finish_frame_send(msg_id, sent);
        if outcome.started_lost_clock || finished.as_ref().is_ok_and(|&became_head| became_head) {
            self.wake_io();
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
                let error = state.closed_error().unwrap_or_else(|| e.into());
                self.release(state);
                return Err(error);
            }
        };
        let answered = state.record_sent_to(msg_id, sent_to, now);
        let became_head = state.inflight.is_head(msg_id);
        self.release(state);
        if let Some(answered) = answered {
            answered.deliver();
        }
        Ok(became_head)
    }

    pub(crate) async fn send_heartbeat(&self) -> Result<(), Error> {
        let msg_id = {
            let mut state = self.lock_state();
            if let Some(error) = self.closed_error_of(&state) {
                return Err(error);
            }
            state.next_msg_id()
        };
        let outcome = self
            .send_to_each_device(
                Kind::Heartbeat,
                msg_id,
                self.heartbeat_resume_device.load(Ordering::Relaxed),
                |_| [&[][..]; 2],
            )
            .await;
        if outcome.started_lost_clock {
            self.wake_io();
        }
        Ok(())
    }

    fn receive_available(&self) -> Result<(), NetworkCause> {
        loop {
            let reply = self.channel.try_recv_from_device().map_err(|e| {
                tracing::error!("receiving failed: {e}");
                NetworkCause::new(UdpError::from(e))
            })?;
            let Some(reply) = reply else {
                return Ok(());
            };
            let mut state = self.lock_state();
            let answered = state.receive_reply(reply, Instant::now());
            self.release(state);
            match answered {
                Ok(Some(answered)) => answered.deliver(),
                Ok(None) => {}
                Err(error) => {
                    self.close(error);
                    return Ok(());
                }
            }
        }
    }

    fn step(&self) -> Result<Instant, NetworkCause> {
        self.receive_available()?;
        let now = Instant::now();
        let lost_device = self
            .lock_state()
            .first_device_unanswered_past_lost_timeout(now);
        if let Some(device) = lost_device {
            self.close(Error::DeviceLost { device });
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
        self.slot_released.notify_one();
    }

    pub(crate) async fn run_io(&self) -> Result<(), NetworkCause> {
        while self.closed_error_of(&self.lock_state()).is_none() {
            let deadline = self.step()?;
            tokio::select! {
                ready = self.channel.readable() => ready.map_err(|e| {
                    tracing::error!("waiting for a reply failed: {e}");
                    NetworkCause::new(UdpError::from(e))
                })?,
                () = self.io_wake.notified() => {}
                () = tokio::time::sleep_until(deadline.into()) => {}
            }
        }
        Ok(())
    }

    pub(crate) fn close(&self, error: Error) {
        let failed = self.lock_state().close(error);
        failed.resolve();
        self.slot_released.notify_one();
        self.wake_io();
        tracing::debug!("session closed");
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
    use std::sync::Arc;

    use autd3_cpu_wire::udp::{Flags, FrameReply, Header};
    use autd3_rs_core::DeviceState;
    use zerocopy::FromBytes;
    use zerocopy::little_endian::U64;

    use crate::commands::Distribution;
    use crate::datagram::Datagram;
    use crate::protocol::Cmd;

    use super::*;

    const TIMEOUTS: Timeouts = Timeouts {
        ack: Duration::from_millis(10),
        lost: Duration::from_millis(100),
    };
    const READY: Flags = Flags::ASSIGNED
        .union(Flags::PTP_LOCKED)
        .union(Flags::SYNC_READY);

    fn session_state(num_devices: usize, now: Instant) -> SessionState {
        SessionState::new(
            num_devices,
            TIMEOUTS,
            DeviceClock::default(),
            DeviceLiveness::new(),
            now,
        )
    }

    fn enqueue(state: &mut SessionState, now: Instant) -> (u16, ResponseFuture) {
        state.try_enqueue(now).unwrap().expect("the queue has room")
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

        assert!(state.try_enqueue(now).unwrap().is_none());

        let head = enqueued[0].0;
        assert!(
            state
                .record_sent_to(head, DeviceSet::from_iter([0]), now)
                .is_none()
        );
        assert!(state.record_reply(reply(0, head, 0), now).is_some());

        assert!(state.try_enqueue(now).unwrap().is_some());
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
        assert!(state.try_enqueue(now).unwrap().is_none());
        enqueued
    }

    #[test]
    fn cancelling_the_newest_frame_frees_its_slot_at_once() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued = fill_and_park(&mut state, now);

        assert!(state.remove(enqueued[MAX_INFLIGHT - 1], now).is_some());

        assert_eq!(state.inflight.requests.len(), MAX_INFLIGHT - 1);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert_eq!(state.inflight.position(msg_id), Some(MAX_INFLIGHT - 1));
    }

    #[test]
    fn a_hole_left_behind_the_head_is_freed_once_the_frames_after_it_are_gone() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued = fill_and_park(&mut state, now);
        assert!(state.remove(enqueued[MAX_INFLIGHT - 2], now).is_some());
        assert!(state.inflight.is_full());

        assert!(state.remove(enqueued[MAX_INFLIGHT - 1], now).is_some());

        assert_eq!(state.inflight.requests.len(), MAX_INFLIGHT - 2);
    }

    #[tokio::test]
    async fn closing_on_a_network_failure_resolves_with_that_failure() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (_msg_id, response) = enqueue(&mut state, now);
        let cause = NetworkCause::new(UdpError::NoInterfaceFound);

        state.close(Error::Network(cause)).resolve();

        assert!(matches!(response.await, Err(Error::Network(_))));
        assert!(matches!(state.closed_error(), Some(Error::Network(_))));
    }

    #[test]
    fn the_first_cause_of_closing_is_kept() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let cause = NetworkCause::new(UdpError::NoInterfaceFound);

        state.close(Error::Network(cause)).resolve();
        state.close(Error::Closed).resolve();

        assert!(matches!(state.closed_error(), Some(Error::Network(_))));
    }

    #[tokio::test]
    async fn closing_resolves_the_pending_frames_and_refuses_new_ones() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (_msg_id, response) = enqueue(&mut state, now);

        state.close(Error::Closed).resolve();

        assert!(matches!(response.await, Err(Error::Closed)));
        assert!(matches!(state.try_enqueue(now), Err(Error::Closed)));
    }

    #[test]
    fn a_frame_answered_before_its_send_returns_is_handed_back_when_the_send_does() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(reply(0, msg_id, 0), now).is_none());
        assert!(state.record_reply(reply(1, msg_id, 0), now).is_none());

        let answered = state.record_sent_to(msg_id, DeviceSet::from_iter([0, 1]), now);

        assert_eq!(answered.map(|answered| answered.msg_id), Some(msg_id));
        assert_eq!(state.inflight.requests.len(), 0);
    }

    #[test]
    fn a_frame_still_missing_a_reply_stays_pending_after_its_send_returns() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(reply(0, msg_id, 0), now).is_none());

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
        assert!(state.record_reply(reply(0, msg_id, 0), now).is_none());
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

    #[tokio::test]
    async fn a_repeated_reply_from_a_device_keeps_the_first_one() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, response) = enqueue(&mut state, now);
        assert!(
            state
                .record_sent_to(msg_id, DeviceSet::from_iter([0, 1]), now)
                .is_none()
        );

        assert!(state.record_reply(reply(0, msg_id, 0), now).is_none());
        assert!(state.record_reply(reply(0, msg_id, 5), now).is_none());
        let answered = state.record_reply(reply(1, msg_id, 0), now).unwrap();
        answered.deliver();

        assert_eq!(response.await.unwrap().error_codes(), [0, 0]);
    }

    #[test]
    fn a_reply_nothing_waits_for_is_dropped() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(
            state
                .record_reply(reply(0, msg_id.wrapping_add(1), 0), now)
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
                .record_reply(reply(0, head, 0), head_answered)
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
        assert_eq!(
            state.first_device_unanswered_past_lost_timeout(start + TIMEOUTS.lost),
            None
        );
        assert_eq!(state.lost_deadline(), Some(start + TIMEOUTS.lost * 2));

        state.timeouts.lost = TIMEOUTS.lost / 2;
        assert_eq!(
            state.first_device_unanswered_past_lost_timeout(start + TIMEOUTS.lost / 2),
            Some(1)
        );
    }

    async fn session_with_device(timeouts: Timeouts) -> (Session, std::net::UdpSocket) {
        let device =
            std::net::UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).unwrap();
        let SocketAddr::V6(device_addr) = device.local_addr().unwrap() else {
            panic!("the device is bound to an IPv6 address");
        };
        let mut channel = Channel::open(device_addr, None, None).await.unwrap();
        channel.attach_devices(vec![device_addr]);
        (
            Session::new(
                channel,
                timeouts,
                DeviceClock::default(),
                DeviceLiveness::new(),
            ),
            device,
        )
    }

    fn answer_the_last_request(device: &std::net::UdpSocket) {
        answer_the_last_request_with(device, READY);
    }

    fn answer_the_last_request_with(device: &std::net::UdpSocket, flags: Flags) {
        let mut buf = [0; 64];
        let (_, host) = device.recv_from(&mut buf).unwrap();
        let (header, _) = Header::read_from_prefix(&buf).unwrap();
        let reply = FrameReply {
            reserved: 0,
            status: 0,
            flags,
            unit_id: 0,
            sys_time: U64::new(0),
        };
        let datagram = [header.as_bytes(), reply.as_bytes()].concat();
        device.send_to(&datagram, host).unwrap();
    }

    #[tokio::test]
    async fn a_device_unanswered_past_the_lost_timeout_closes_the_session_and_nothing_is_sent_after()
     {
        let (session, device) = session_with_device(Timeouts {
            ack: Duration::from_secs(3600),
            lost: Duration::ZERO,
        })
        .await;
        let datagrams = [Datagram::no_payload(Cmd::Nop)];
        let frame = Frame::new(Distribution::Broadcast, &datagrams);
        let (enqueued, response) = session.enqueue().await.unwrap();
        session.send_frame(enqueued, frame, 0).await.unwrap();
        device.recv_from(&mut [0; 64]).unwrap();

        session.step().unwrap();

        assert!(matches!(
            response.await,
            Err(Error::DeviceLost { device: 0 })
        ));
        assert!(matches!(
            session.send_heartbeat().await,
            Err(Error::DeviceLost { device: 0 })
        ));
        assert!(matches!(
            session.enqueue().await.map(|_| ()),
            Err(Error::DeviceLost { device: 0 })
        ));
        device.set_nonblocking(true).unwrap();
        assert_eq!(
            device.recv_from(&mut [0; 64]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[tokio::test]
    async fn a_loss_and_an_ack_timeout_in_the_same_step_close_the_session_as_lost() {
        let (session, device) = session_with_device(Timeouts {
            ack: Duration::ZERO,
            lost: Duration::ZERO,
        })
        .await;
        let datagrams = [Datagram::no_payload(Cmd::Nop)];
        let frame = Frame::new(Distribution::Broadcast, &datagrams);
        let (enqueued, response) = session.enqueue().await.unwrap();
        session.send_frame(enqueued, frame, 0).await.unwrap();
        device.recv_from(&mut [0; 64]).unwrap();

        session.step().unwrap();

        assert!(matches!(
            response.await,
            Err(Error::DeviceLost { device: 0 })
        ));
    }

    #[tokio::test]
    async fn the_io_loop_ends_once_the_session_is_closed_by_a_lost_device() {
        let (session, _device) = session_with_device(TIMEOUTS).await;

        session.close(Error::DeviceLost { device: 0 });

        assert!(session.run_io().await.is_ok());
        assert!(!session.is_stopping());
    }

    #[tokio::test]
    async fn an_unassigned_reply_closes_the_session() {
        let (session, device) = session_with_device(TIMEOUTS).await;
        let datagrams = [Datagram::no_payload(Cmd::Nop)];
        let frame = Frame::new(Distribution::Broadcast, &datagrams);
        let (enqueued, response) = session.enqueue().await.unwrap();
        session.send_frame(enqueued, frame, 0).await.unwrap();
        answer_the_last_request_with(&device, Flags::PTP_LOCKED | Flags::SYNC_READY);
        session.channel.readable().await.unwrap();

        session.receive_available().unwrap();

        assert!(matches!(
            response.await,
            Err(Error::DeviceLost { device: 0 })
        ));
        assert!(matches!(
            session.lock_state().closed_error(),
            Some(Error::DeviceLost { device: 0 })
        ));
    }

    #[tokio::test]
    async fn dropping_an_unfinished_frame_gives_its_slot_back() {
        let (session, _device) = session_with_device(TIMEOUTS).await;
        let mut enqueued = Vec::new();
        for _ in 0..MAX_INFLIGHT {
            enqueued.push(session.enqueue().await.unwrap());
        }
        assert!(session.lock_state().inflight.is_full());

        drop(enqueued.pop());

        assert_eq!(
            session.lock_state().inflight.requests.len(),
            MAX_INFLIGHT - 1
        );
        session.enqueue().await.unwrap();
    }

    #[tokio::test]
    async fn dropping_the_future_of_a_frame_send_keeps_later_sends_going() {
        let (session, device) = session_with_device(TIMEOUTS).await;
        let datagrams = [Datagram::no_payload(Cmd::Nop)];
        let frame = Frame::new(Distribution::Broadcast, &datagrams);
        let (enqueued, _response) = session.enqueue().await.unwrap();

        drop(session.send_frame(enqueued, frame, 0));

        assert_eq!(session.lock_state().inflight.requests.len(), 0);
        let (enqueued, response) = session.enqueue().await.unwrap();
        session.send_frame(enqueued, frame, 0).await.unwrap();
        answer_the_last_request(&device);
        session.channel.readable().await.unwrap();
        session.receive_available().unwrap();
        assert!(response.await.is_ok());
    }

    #[tokio::test]
    async fn a_parked_sender_takes_the_slot_a_finished_frame_releases() {
        let session = Arc::new(session_with_device(TIMEOUTS).await.0);
        let mut enqueued = Vec::new();
        for _ in 0..MAX_INFLIGHT {
            enqueued.push(session.enqueue().await.unwrap());
        }
        let parked = tokio::spawn({
            let session = Arc::clone(&session);
            async move { session.enqueue().await.is_ok() }
        });
        tokio::task::yield_now().await;
        assert!(!parked.is_finished());

        drop(enqueued.pop());

        assert!(parked.await.unwrap());
    }

    #[tokio::test]
    async fn failing_every_pending_frame_releases_the_parked_sender() {
        let timeouts = Timeouts {
            ack: Duration::ZERO,
            lost: Duration::from_secs(3600),
        };
        let session = Arc::new(session_with_device(timeouts).await.0);
        let mut enqueued = Vec::new();
        for _ in 0..MAX_INFLIGHT {
            enqueued.push(session.enqueue().await.unwrap());
        }
        session.lock_state().record_sent_to(
            enqueued[0].0.msg_id(),
            DeviceSet::from_iter([0]),
            Instant::now(),
        );
        let parked = tokio::spawn({
            let session = Arc::clone(&session);
            async move { session.enqueue().await.is_ok() }
        });
        tokio::task::yield_now().await;

        session.step().unwrap();

        assert!(parked.await.unwrap());
    }

    #[tokio::test]
    async fn closing_releases_a_parked_sender() {
        let session = Arc::new(session_with_device(TIMEOUTS).await.0);
        let mut enqueued = Vec::new();
        for _ in 0..MAX_INFLIGHT {
            enqueued.push(session.enqueue().await.unwrap());
        }
        let parked = tokio::spawn({
            let session = Arc::clone(&session);
            async move { session.enqueue().await.map(|_| ()) }
        });
        tokio::task::yield_now().await;

        session.close(Error::Closed);

        assert!(matches!(parked.await.unwrap(), Err(Error::Closed)));
        drop(enqueued);
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

        state
            .receive_reply(timed_reply(1, 1, DEVICE_TIME_NS), now)
            .unwrap();
        assert!(state.device_clock.now(now).is_none());
        state.receive_reply(timed_reply(0, 0, 0), now).unwrap();
        assert!(state.device_clock.now(now).is_none());
        state
            .receive_reply(timed_reply(0, 0, DEVICE_TIME_NS), now)
            .unwrap();
        assert!(state.device_clock.now(now).is_some());
    }

    #[test]
    fn a_reply_under_a_foreign_id_observes_nothing() {
        let now = Instant::now();
        let mut state = session_state(1, now);

        assert!(matches!(
            state.receive_reply(timed_reply(0, 1, DEVICE_TIME_NS), now),
            Err(Error::DeviceLost { device: 0 })
        ));
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
        state
            .receive_reply(flagged_reply(1, 1, Flags::ASSIGNED), now)
            .unwrap();

        let status = state.device_status();

        assert_eq!(
            status.iter().map(|s| s.state).collect::<Vec<_>>(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Ready]
        );
        assert_eq!(
            status.iter().map(|s| s.ptp_locked).collect::<Vec<_>>(),
            [true, false, true]
        );
        assert!(status.iter().all(|s| !s.failsafe_active));
    }

    #[test]
    fn the_device_status_reports_the_flags_of_the_last_reply() {
        let now = Instant::now();
        let mut state = session_state(3, now);
        state
            .receive_reply(
                flagged_reply(0, 0, Flags::ASSIGNED | Flags::PTP_LOCKED),
                now,
            )
            .unwrap();
        state
            .receive_reply(
                flagged_reply(1, 1, Flags::ASSIGNED | Flags::FAILSAFE | Flags::SYNC_READY),
                now,
            )
            .unwrap();
        state
            .receive_reply(flagged_reply(2, 2, READY | Flags::FAILSAFE), now)
            .unwrap();

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

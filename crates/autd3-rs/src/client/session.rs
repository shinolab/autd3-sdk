use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Kind;
use autd3_rs_core::{BusStats, DeviceClock, MAX_DEVICES};
use futures_channel::oneshot;
use zerocopy::{FromZeros, IntoBytes};

use crate::datagram::Frame;
use crate::error::{Error, NetworkCause};
use crate::geometry::Geometry;
use crate::protocol::{FrameHeader, MAX_INFLIGHT};
use crate::response::{ReplyBuffers, Response};
use crate::udp::state::PublishedDeviceStates;
use crate::udp::{Channel, DeviceReply, Received, Reply, StateChecker, TransportOption, UdpError};
use crate::value::SysTime;

use super::completion::ResponseSender;
use super::liveness::DeviceLiveness;
use super::recovery::RecoveryWindow;

const IDLE_WAIT: Duration = Duration::from_secs(1);

pub(crate) type RecoveryResultSender = oneshot::Sender<Result<u128, Error>>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Timeouts {
    pub(crate) ack: Duration,
    pub(crate) lost: Duration,
    pub(crate) recovery_reply: Duration,
}

struct InflightRequest {
    msg_id: u16,
    sent_to: Option<u128>,
    replied_devices: u128,
    sent_at: Instant,
    completion: ResponseSender,
}

impl InflightRequest {
    fn has_all_replies(&self) -> bool {
        self.sent_to
            .is_some_and(|sent_to| self.replied_devices & sent_to == sent_to)
    }
}

pub(crate) struct AnsweredRequest {
    request: InflightRequest,
    response: Response,
}

impl AnsweredRequest {
    fn deliver(self, stats: &BusStats) {
        tracing::trace!(
            msg_id = self.request.msg_id,
            "frame answered by every device"
        );
        stats.record_ack(
            u64::try_from(self.request.sent_at.elapsed().as_nanos()).unwrap_or(u64::MAX),
        );
        if let Err(e) = self.response.check() {
            tracing::warn!(error = %e, "device reported an error");
        }
        let _ = self.request.completion.send(Ok(self.response));
    }
}

struct InflightSlot {
    request: Option<InflightRequest>,
    reply: ReplyBuffers,
}

struct InflightQueue {
    slots: Box<[InflightSlot]>,
    head: usize,
    len: usize,
    head_ack_clock_started_at: Instant,
    slot_waiter: Option<Waker>,
}

impl InflightQueue {
    fn new(num_devices: usize, now: Instant) -> Self {
        Self {
            slots: (0..MAX_INFLIGHT)
                .map(|_| InflightSlot {
                    request: None,
                    reply: ReplyBuffers::new(num_devices),
                })
                .collect(),
            head: 0,
            len: 0,
            head_ack_clock_started_at: now,
            slot_waiter: None,
        }
    }

    fn is_full(&self) -> bool {
        self.len == self.slots.len()
    }

    fn slot_index(&self, offset: usize) -> usize {
        (self.head + offset) % self.slots.len()
    }

    fn push(&mut self, request: InflightRequest) {
        if self.len == 0 {
            self.head_ack_clock_started_at = request.sent_at;
        }
        let index = self.slot_index(self.len);
        let slot = &mut self.slots[index];
        slot.reply.reset();
        slot.request = Some(request);
        self.len += 1;
    }

    fn head(&self) -> Option<&InflightRequest> {
        if self.len == 0 {
            return None;
        }
        self.slots[self.head].request.as_ref()
    }

    fn is_head(&self, msg_id: u16) -> bool {
        self.head().is_some_and(|head| head.msg_id == msg_id)
    }

    fn position(&self, msg_id: u16) -> Option<usize> {
        (0..self.len)
            .map(|offset| self.slot_index(offset))
            .find(|&index| {
                self.slots[index]
                    .request
                    .as_ref()
                    .is_some_and(|request| request.msg_id == msg_id)
            })
    }

    fn take(&mut self, index: usize, now: Instant) -> Option<InflightRequest> {
        let request = self.slots[index].request.take();
        while self.len > 0 && self.slots[self.head].request.is_none() {
            self.head = (self.head + 1) % self.slots.len();
            self.len -= 1;
            self.head_ack_clock_started_at = now;
        }
        while self.len > 0 && self.slots[self.slot_index(self.len - 1)].request.is_none() {
            self.len -= 1;
        }
        request
    }

    fn take_all(&mut self) -> Vec<InflightRequest> {
        let requests = (0..self.len)
            .filter_map(|offset| {
                let index = self.slot_index(offset);
                self.slots[index].request.take()
            })
            .collect();
        self.len = 0;
        requests
    }

    fn head_ack_deadline(&self, ack_timeout: Duration) -> Option<Instant> {
        self.head()
            .filter(|head| head.sent_to.is_some())
            .and_then(|_| self.head_ack_clock_started_at.checked_add(ack_timeout))
    }
}

pub(crate) struct ReplyAcceptance {
    accepted: bool,
    revived: bool,
}

pub(crate) struct SessionState {
    inflight: InflightQueue,
    liveness: Vec<DeviceLiveness>,
    published_states: Arc<PublishedDeviceStates>,
    recovery_window: Option<RecoveryWindow>,
    recovery_waiter: Option<RecoveryResultSender>,
    network_failure: Option<NetworkCause>,
    is_closed: bool,
    last_msg_id: u16,
    timeouts: Timeouts,
}

pub(crate) struct ClosedSession {
    requests: Vec<InflightRequest>,
    recovery_waiter: Option<RecoveryResultSender>,
    slot_waiter: Option<Waker>,
    network_failure: Option<NetworkCause>,
}

impl SessionState {
    pub(crate) fn new(
        num_devices: usize,
        timeouts: Timeouts,
        published_states: Arc<PublishedDeviceStates>,
        now: Instant,
    ) -> Self {
        Self {
            inflight: InflightQueue::new(num_devices, now),
            liveness: vec![DeviceLiveness::new(); num_devices],
            published_states,
            recovery_window: None,
            recovery_waiter: None,
            network_failure: None,
            is_closed: false,
            last_msg_id: 0,
            timeouts,
        }
    }

    pub(crate) fn closed_error(&self) -> Option<Error> {
        if let Some(cause) = &self.network_failure {
            return Some(Error::Network(cause.clone()));
        }
        self.is_closed.then_some(Error::Closed)
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
            replied_devices: 0,
            sent_at: now,
            completion: completion
                .take()
                .expect("an enqueued frame is not polled again"),
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

    fn answer(&mut self, index: usize, now: Instant) -> Option<AnsweredRequest> {
        let replied_devices = self.inflight.slots[index].request.as_ref()?.replied_devices;
        let response = self.inflight.slots[index].reply.response(replied_devices);
        let request = self.inflight.take(index, now)?;
        Some(AnsweredRequest { request, response })
    }

    pub(crate) fn record_sent_to(
        &mut self,
        msg_id: u16,
        sent_to: u128,
        now: Instant,
    ) -> Option<AnsweredRequest> {
        let index = self.inflight.position(msg_id)?;
        let request = self.inflight.slots[index].request.as_mut()?;
        request.sent_to = Some(sent_to);
        if request.has_all_replies() {
            return self.answer(index, now);
        }
        if index == self.inflight.head {
            self.inflight.head_ack_clock_started_at = now;
        }
        None
    }

    pub(crate) fn record_reply(&mut self, reply: &Reply, now: Instant) -> Option<AnsweredRequest> {
        let Some(index) = self.inflight.position(reply.msg_id) else {
            tracing::trace!(msg_id = reply.msg_id, "dropping a reply nothing waits for");
            return None;
        };
        let reply_bit = 1u128 << reply.device;
        let slot = &mut self.inflight.slots[index];
        let request = slot.request.as_mut()?;
        if request.replied_devices & reply_bit != 0 {
            tracing::trace!(
                msg_id = reply.msg_id,
                device = reply.device,
                "dropping a repeated reply"
            );
            return None;
        }
        slot.reply
            .record_reply(reply.device, reply.error_code, reply.sys_time, reply.data);
        request.replied_devices |= reply_bit;
        if !request.has_all_replies() {
            return None;
        }
        self.answer(index, now)
    }

    fn device_mask_where(&self, holds: impl Fn(DeviceLiveness) -> bool) -> u128 {
        self.liveness
            .iter()
            .enumerate()
            .filter(|&(_, &liveness)| holds(liveness))
            .fold(0u128, |mask, (device, _)| mask | 1 << device)
    }

    fn publish(&self, device: usize) {
        let liveness = self.liveness[device];
        self.published_states
            .publish(device, liveness.state(), liveness.ptp_flags());
    }

    fn start_unanswered_clock(
        &mut self,
        device: usize,
        now: Instant,
        previous: Option<Instant>,
    ) -> bool {
        let liveness = &mut self.liveness[device];
        if let Some(previous) = previous {
            liveness.cancel_unanswered_clock(previous);
        }
        liveness.start_unanswered_clock(now)
    }

    fn record_send_failure(&mut self, device: usize, now: Instant) {
        self.liveness[device].record_send_failure(now, self.timeouts.lost);
        self.publish(device);
    }

    pub(crate) fn accept_reply(&mut self, reply: &DeviceReply) -> ReplyAcceptance {
        let device = reply.reply.device;
        let expected_unit_id = u8::try_from(device).expect("a device index is below MAX_DEVICES");
        let liveness = &mut self.liveness[device];
        let accepted = liveness.accept_reply(expected_unit_id, reply.reported_unit_id, reply.flags);
        let revived = !accepted
            && self.recovery_window.is_some()
            && liveness.try_revive(expected_unit_id, reply.reported_unit_id, reply.flags);
        if revived && let Some(window) = &mut self.recovery_window {
            window.mark_recovered(device);
        }
        self.publish(device);
        ReplyAcceptance { accepted, revived }
    }

    pub(crate) fn mark_unanswered_devices_lost(&mut self, now: Instant) {
        for device in 0..self.liveness.len() {
            self.liveness[device].mark_lost_if_unanswered(now, self.timeouts.lost);
            self.publish(device);
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
            self.recovery_window
                .as_ref()
                .and_then(RecoveryWindow::deadline),
            self.lost_deadline(),
            self.inflight.head_ack_deadline(self.timeouts.ack),
        ]
        .into_iter()
        .flatten()
        .fold(now + IDLE_WAIT, Instant::min)
    }

    pub(crate) fn start_recovery(
        &mut self,
        waiter: RecoveryResultSender,
        now: Instant,
    ) -> Result<Option<u16>, Error> {
        if let Some(error) = self.closed_error() {
            return Err(error);
        }
        self.recovery_waiter = Some(waiter);
        let lost_devices = self.device_mask_where(DeviceLiveness::is_lost);
        let window = self.timeouts.recovery_reply;
        match &mut self.recovery_window {
            Some(open) => open.restart_with(now, window, lost_devices),
            none @ None => *none = Some(RecoveryWindow::new(now, window, lost_devices)),
        }
        Ok((lost_devices != 0).then(|| self.next_msg_id()))
    }

    pub(crate) fn take_finished_recovery(
        &mut self,
        now: Instant,
    ) -> Option<(Option<RecoveryResultSender>, u128)> {
        if !self.recovery_window.as_ref()?.is_finished(now) {
            return None;
        }
        let recovered_devices = self.recovery_window.take()?.recovered_devices();
        Some((self.recovery_waiter.take(), recovered_devices))
    }

    pub(crate) fn readmit_recovered(&mut self) {
        for liveness in &mut self.liveness {
            liveness.readmit();
        }
    }

    pub(crate) fn close(&mut self, network_failure: Option<&NetworkCause>) -> ClosedSession {
        self.network_failure = network_failure.cloned();
        self.is_closed = true;
        self.published_states.close();
        ClosedSession {
            requests: self.inflight.take_all(),
            recovery_waiter: self.recovery_waiter.take(),
            slot_waiter: self.inflight.slot_waiter.take(),
            network_failure: self.network_failure.clone(),
        }
    }
}

impl ClosedSession {
    fn resolve(self) {
        let error = || {
            self.network_failure
                .clone()
                .map_or(Error::Closed, Error::Network)
        };
        if let Some(recovery_waiter) = self.recovery_waiter {
            let _ = recovery_waiter.send(Err(error()));
        }
        for request in self.requests {
            let _ = request.completion.send(Err(error()));
        }
        if let Some(slot_waiter) = self.slot_waiter {
            slot_waiter.wake();
        }
    }
}

enum Incoming {
    Wake,
    Reply(DeviceReply),
    Irrelevant,
}

pub(crate) struct Session {
    channel: Channel,
    state: Mutex<SessionState>,
    stop_requested: AtomicBool,
    heartbeat_resume_device: AtomicUsize,
    published_states: Arc<PublishedDeviceStates>,
    device_clock: DeviceClock,
    stats: BusStats,
    timeouts: Timeouts,
}

impl Session {
    pub(crate) fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, UdpError> {
        let device_clock = DeviceClock::new();
        let channel = super::open::open(option, geometry.num_devices(), &device_clock)?;
        let num_devices = channel.num_devices();
        let timeouts = Timeouts {
            ack: option.ack_timeout,
            lost: option.lost_timeout,
            recovery_reply: option.recovery_reply_timeout,
        };
        let published_states = PublishedDeviceStates::new(num_devices);
        Ok(Self {
            state: Mutex::new(SessionState::new(
                num_devices,
                timeouts,
                Arc::clone(&published_states),
                Instant::now(),
            )),
            stop_requested: AtomicBool::new(false),
            heartbeat_resume_device: AtomicUsize::new(num_devices - 1),
            published_states,
            channel,
            device_clock,
            stats: BusStats::default(),
            timeouts,
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

    pub(crate) fn stats(&self) -> BusStats {
        self.stats.clone()
    }

    pub(crate) fn state_checker(&self) -> StateChecker {
        StateChecker::new(Arc::clone(&self.published_states))
    }

    pub(crate) fn device_time(&self) -> Option<SysTime> {
        self.device_clock.now()
    }

    pub(crate) fn lost_timeout(&self) -> Duration {
        self.timeouts.lost
    }

    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }

    pub(crate) fn closed_error(&self) -> Option<Error> {
        let error = self.lock_state().closed_error();
        if self.is_stopping() {
            return Some(error.unwrap_or(Error::Closed));
        }
        error
    }

    pub(crate) fn network_failure(&self) -> Option<NetworkCause> {
        self.lock_state().network_failure.clone()
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
        target_devices: u128,
        start_device: usize,
        body_for_device: impl Fn(usize) -> [&'a [u8]; 2],
    ) -> (Result<u128, UdpError>, bool) {
        let mut retry_full_socket_until = match kind {
            Kind::Frame => Instant::now().checked_add(self.timeouts.lost),
            _ => None,
        };
        let mut accepted_devices = 0u128;
        let mut first_failure = None;
        let mut started_lost_clock = false;
        let num_devices = self.channel.num_devices();
        for device in (0..num_devices).map(|step| (start_device + num_devices - step) % num_devices)
        {
            if target_devices & (1 << device) == 0 {
                continue;
            }
            let mut attempted_at = None;
            let result = self.channel.send_to_device(
                device,
                kind,
                msg_id,
                body_for_device(device),
                retry_full_socket_until,
                || {
                    let now = Instant::now();
                    started_lost_clock |= self.lock_state().start_unanswered_clock(
                        device,
                        now,
                        attempted_at.replace(now),
                    );
                },
            );
            if result.is_err()
                && let Some(attempted_at) = attempted_at
            {
                self.lock_state().liveness[device].cancel_unanswered_clock(attempted_at);
            }
            match result {
                Ok(()) => accepted_devices |= 1 << device,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if kind == Kind::Frame {
                        if retry_full_socket_until.take().is_some() {
                            tracing::warn!(
                                device,
                                "the socket stayed full past the lost timeout; the frame goes unanswered"
                            );
                        }
                        accepted_devices |= 1 << device;
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
        let sent = match first_failure {
            Some(failure) if kind == Kind::Frame => Err(failure),
            _ => Ok(accepted_devices),
        };
        (sent, started_lost_clock)
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
            .device_mask_where(DeviceLiveness::accepts_frames);
        let mut headers = [FrameHeader::new_zeroed(); MAX_DEVICES];
        for (header, datagram) in headers.iter_mut().zip(datagrams) {
            *header = datagram.frame_header(exec_at);
        }
        let (sent, started_lost_clock) = self.send_to_each_device(
            Kind::Frame,
            msg_id,
            target_devices,
            self.channel.num_devices() - 1,
            |device| {
                let index = frame.datagram_index(device);
                [headers[index].as_bytes(), datagrams[index].payload()]
            },
        );
        let finished = self.finish_frame_send(msg_id, sent);
        if finished.as_ref().is_ok_and(|&became_head| became_head) {
            self.wake_driver("the ack timeout is noticed at its next deadline");
        } else if started_lost_clock {
            self.wake_driver("a silent device is noticed at its next deadline");
        }
        finished.map(|_| ())
    }

    fn finish_frame_send(&self, msg_id: u16, sent: Result<u128, UdpError>) -> Result<bool, Error> {
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
        if sent_to == 0 {
            let completion = state.remove(msg_id, now);
            Self::release(state);
            if let Some(completion) = completion {
                let _ = completion.send(Err(Error::Timeout {
                    timeout: self.timeouts.ack,
                }));
            }
            return Ok(false);
        }
        let answered = state.record_sent_to(msg_id, sent_to, now);
        let became_head = state.inflight.is_head(msg_id);
        Self::release(state);
        if let Some(answered) = answered {
            answered.deliver(&self.stats);
        }
        self.stats.record_frame();
        Ok(became_head)
    }

    pub(crate) fn send_heartbeat(&self) -> Result<(), Error> {
        let (msg_id, target_devices) = {
            let mut state = self.lock_state();
            if let Some(error) = state.closed_error() {
                return Err(error);
            }
            (
                state.next_msg_id(),
                state.device_mask_where(|liveness| !liveness.is_lost()),
            )
        };
        let (sent, started_lost_clock) = self.send_to_each_device(
            Kind::Heartbeat,
            msg_id,
            target_devices,
            self.heartbeat_resume_device.load(Ordering::Relaxed),
            |_| [&[][..]; 2],
        );
        if started_lost_clock {
            self.wake_driver("a silent device is noticed at its next deadline");
        }
        if sent? != 0 {
            self.stats.record_heartbeat();
        }
        Ok(())
    }

    pub(crate) fn start_recovery(&self, waiter: RecoveryResultSender) -> Result<(), Error> {
        let probe_msg_id = self.lock_state().start_recovery(waiter, Instant::now())?;
        if let Some(msg_id) = probe_msg_id {
            let all_devices_dst = self.channel.all_devices_dst();
            if let Err(e) = self.channel.send(
                all_devices_dst,
                Kind::Heartbeat,
                msg_id,
                [&[], &[]],
                None,
                || {},
            ) {
                tracing::debug!(%all_devices_dst, "calling the lost devices back failed: {e}");
            }
        }
        self.wake_driver("the recovery finishes at its next deadline");
        Ok(())
    }

    pub(crate) fn readmit_recovered(&self) {
        self.lock_state().readmit_recovered();
    }

    fn receive_available(&self) -> Result<(), NetworkCause> {
        loop {
            let incoming = self
                .channel
                .recv_from_device(Instant::now(), |received| match received {
                    Received::Wake => Incoming::Wake,
                    Received::Device {
                        device,
                        header,
                        body,
                    } => DeviceReply::parse(device, header, body)
                        .map_or(Incoming::Irrelevant, Incoming::Reply),
                    Received::Other => Incoming::Irrelevant,
                })
                .map_err(|e| {
                    tracing::error!("receiving failed: {e}");
                    NetworkCause::new(UdpError::from(e))
                })?;
            match incoming {
                None => {
                    self.lock_state()
                        .mark_unanswered_devices_lost(Instant::now());
                    return Ok(());
                }
                Some(Incoming::Wake) => return Ok(()),
                Some(Incoming::Irrelevant) => {}
                Some(Incoming::Reply(reply)) => self.handle_reply(&reply),
            }
        }
    }

    fn handle_reply(&self, reply: &DeviceReply) {
        let mut state = self.lock_state();
        let ReplyAcceptance { accepted, revived } = state.accept_reply(reply);
        if !(accepted || revived) {
            return;
        }
        if reply.reply.device == 0
            && let Some(sys_time) = NonZeroU64::new(reply.reply.sys_time)
        {
            self.device_clock
                .observe(SysTime::from_nanos(sys_time.get()));
        }
        let answered = state.record_reply(&reply.reply, Instant::now());
        Self::release(state);
        if revived {
            self.stats.record_recovery();
            tracing::info!(device = reply.reply.device, "a lost device answers again");
        }
        if let Some(answered) = answered {
            answered.deliver(&self.stats);
        }
    }

    pub(crate) fn step(&self) -> Result<Instant, NetworkCause> {
        self.receive_available()?;
        let now = Instant::now();
        if self.lock_state().is_head_ack_overdue(now) {
            self.receive_available()?;
        }
        self.fail_all_if_head_ack_overdue(now);
        let finished_recovery = self.lock_state().take_finished_recovery(now);
        if let Some((Some(waiter), recovered_devices)) = finished_recovery {
            let _ = waiter.send(Ok(recovered_devices));
        }
        Ok(self.lock_state().next_deadline(now))
    }

    fn fail_all_if_head_ack_overdue(&self, now: Instant) {
        let mut state = self.lock_state();
        if !state.is_head_ack_overdue(now) {
            return;
        }
        let requests = state.inflight.take_all();
        Self::release(state);
        tracing::warn!(
            pending = requests.len(),
            "the head frame got no reply within the ack timeout; failing the pending frames"
        );
        for request in requests {
            let _ = request.completion.send(Err(Error::Timeout {
                timeout: self.timeouts.ack,
            }));
        }
    }

    pub(crate) fn close(&self, network_failure: Option<&NetworkCause>) {
        let closed = self.lock_state().close(network_failure);
        closed.resolve();
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
        recovery_reply: Duration::from_millis(1),
    };
    const READY: Flags = Flags::ASSIGNED
        .union(Flags::PTP_LOCKED)
        .union(Flags::SYNC_READY);

    fn session_state(num_devices: usize, now: Instant) -> SessionState {
        SessionState::new(
            num_devices,
            TIMEOUTS,
            PublishedDeviceStates::new(num_devices),
            now,
        )
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
        assert!(state.record_sent_to(head, 0b1, now).is_none());
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
        assert_eq!(state.inflight.len, MAX_INFLIGHT - 2);
        assert!(state.inflight.is_head(enqueued[2].0));
    }

    #[test]
    fn the_ring_wraps_around_and_keeps_the_order() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        for _ in 0..MAX_INFLIGHT * 2 + 3 {
            let (msg_id, _response) = enqueue(&mut state, now);
            assert!(state.inflight.is_head(msg_id));
            assert!(state.remove(msg_id, now).is_some());
            assert_eq!(state.inflight.len, 0);
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

        assert_eq!(state.inflight.len, MAX_INFLIGHT - 1);
        assert!(state.take_released_slot_waiter().is_some());
        let (msg_id, _response) = enqueue(&mut state, now);
        assert_eq!(
            state.inflight.position(msg_id),
            Some(state.inflight.slot_index(MAX_INFLIGHT - 1))
        );
    }

    #[test]
    fn a_hole_left_behind_the_head_is_freed_once_the_frames_after_it_are_gone() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let enqueued = fill_and_park(&mut state, now);
        assert!(state.remove(enqueued[MAX_INFLIGHT - 2], now).is_some());
        assert!(state.take_released_slot_waiter().is_none());

        assert!(state.remove(enqueued[MAX_INFLIGHT - 1], now).is_some());

        assert_eq!(state.inflight.len, MAX_INFLIGHT - 2);
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
        let cause = NetworkCause::new(UdpError::Closed);

        state.close(Some(&cause)).resolve();

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

        let closed = state.close(None);
        assert!(closed.slot_waiter.is_some());
        closed.resolve();

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

        let answered = state.record_sent_to(msg_id, 0b11, now);

        assert_eq!(
            answered.map(|answered| answered.request.msg_id),
            Some(msg_id)
        );
        assert_eq!(state.inflight.len, 0);
    }

    #[test]
    fn a_frame_still_missing_a_reply_stays_pending_after_its_send_returns() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());

        assert!(state.record_sent_to(msg_id, 0b11, now).is_none());

        assert_eq!(state.inflight.len, 1);
        assert_eq!(state.inflight.head().unwrap().sent_to, Some(0b11));
    }

    #[test]
    fn a_frame_is_never_answered_while_its_send_is_still_running() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());
        assert_eq!(state.inflight.len, 1);
    }

    #[test]
    fn only_the_frame_at_the_head_asks_for_a_wake() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (head, _head_response) = enqueue(&mut state, now);
        let (next, _next_response) = enqueue(&mut state, now);
        assert!(state.record_sent_to(head, 0b1, now).is_none());
        assert!(state.record_sent_to(next, 0b1, now).is_none());

        assert!(state.inflight.is_head(head));
        assert!(!state.inflight.is_head(next));
    }

    #[test]
    fn a_repeated_reply_from_a_device_keeps_the_first_one() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        let (msg_id, response) = enqueue(&mut state, now);
        assert!(state.record_sent_to(msg_id, 0b11, now).is_none());

        assert!(state.record_reply(&reply(0, msg_id, 0), now).is_none());
        assert!(state.record_reply(&reply(0, msg_id, 5), now).is_none());
        let answered = state.record_reply(&reply(1, msg_id, 0), now).unwrap();
        answered.deliver(&BusStats::default());

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
        assert_eq!(state.inflight.len, 1);
    }

    #[test]
    fn the_ack_clock_of_the_head_starts_when_its_send_returns_or_it_becomes_the_head() {
        let start = Instant::now();
        let mut state = session_state(1, start);
        let (head, _head_response) = enqueue(&mut state, start);
        let (next, _next_response) = enqueue(&mut state, start);
        assert_eq!(state.inflight.head_ack_deadline(TIMEOUTS.ack), None);

        let head_sent = start + Duration::from_millis(1);
        assert!(state.record_sent_to(head, 0b1, head_sent).is_none());
        let next_sent = start + Duration::from_millis(2);
        assert!(state.record_sent_to(next, 0b1, next_sent).is_none());
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

        state.start_unanswered_clock(0, now, None);
        assert_eq!(state.next_deadline(now), now + TIMEOUTS.lost);

        let (msg_id, _response) = enqueue(&mut state, now);
        assert!(state.record_sent_to(msg_id, 0b1, now).is_none());
        assert_eq!(state.next_deadline(now), now + TIMEOUTS.ack);
    }

    #[test]
    fn a_recovery_is_reported_once_its_window_closes() {
        let now = Instant::now();
        let mut state = session_state(2, now);
        state.start_unanswered_clock(1, now, None);
        state.mark_unanswered_devices_lost(now + TIMEOUTS.lost);
        let (waiter, mut result) = oneshot::channel();

        assert!(state.start_recovery(waiter, now).unwrap().is_some());
        assert!(state.take_finished_recovery(now).is_none());
        assert_eq!(state.next_deadline(now), now + TIMEOUTS.recovery_reply);

        let acceptance = state.accept_reply(&DeviceReply {
            reply: reply(1, 0, 0),
            reported_unit_id: 1,
            flags: READY,
        });
        assert!(acceptance.revived);
        let (waiter, recovered_devices) = state.take_finished_recovery(now).unwrap();
        assert_eq!(recovered_devices, 0b10);
        waiter.unwrap().send(Ok(recovered_devices)).unwrap();
        assert_eq!(result.try_recv().unwrap().unwrap().unwrap(), 0b10);
        assert!(state.recovery_waiter.is_none());

        assert_eq!(
            state.device_mask_where(DeviceLiveness::accepts_frames),
            0b01
        );
        state.readmit_recovered();
        assert_eq!(
            state.device_mask_where(DeviceLiveness::accepts_frames),
            0b11
        );
    }

    #[test]
    fn a_recovery_without_a_lost_device_sends_no_probe_and_finishes_at_once() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        let (waiter, _result) = oneshot::channel();
        assert_eq!(state.start_recovery(waiter, now).unwrap(), None);
        assert_eq!(state.take_finished_recovery(now).unwrap().1, 0);
    }

    #[test]
    fn a_recovery_is_refused_once_closed() {
        let now = Instant::now();
        let mut state = session_state(1, now);
        state.close(None).resolve();
        let (waiter, _result) = oneshot::channel();
        assert!(matches!(
            state.start_recovery(waiter, now),
            Err(Error::Closed)
        ));
        assert!(state.recovery_waiter.is_none());
    }

    #[test]
    fn closing_publishes_the_close_to_the_state_checker() {
        let now = Instant::now();
        let published_states = PublishedDeviceStates::new(1);
        let checker = StateChecker::new(Arc::clone(&published_states));
        let mut state = SessionState::new(1, TIMEOUTS, published_states, now);
        assert!(checker.check().is_ok());
        state.close(None).resolve();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
        assert_eq!(state.liveness[0].state(), DeviceState::Ready);
    }
}

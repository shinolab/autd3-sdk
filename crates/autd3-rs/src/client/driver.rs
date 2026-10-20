use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_rs_core::BusStats;
use futures_channel::oneshot;

use crate::error::{Error, NetworkCause};
use crate::udp::{Reply, Sent, UdpBus, UdpError};

use super::completion::CompletionSender;
use super::pool::ReplySlot;

const IDLE_WAIT: Duration = Duration::from_secs(1);

pub(crate) struct InflightRequest {
    pub(crate) msg_id: u16,
    pub(crate) sent_to: Option<u128>,
    pub(crate) replied_devices: u128,
    pub(crate) sent_at: Instant,
    pub(crate) reply_slot: ReplySlot,
    pub(crate) completion: CompletionSender,
}

impl InflightRequest {
    fn has_all_replies(&self) -> bool {
        self.sent_to
            .is_some_and(|sent_to| self.replied_devices & sent_to == sent_to)
    }

    pub(crate) fn complete(self, stats: &BusStats) {
        tracing::trace!(msg_id = self.msg_id, "frame answered by every device");
        stats.record_ack(u64::try_from(self.sent_at.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let response = self.reply_slot.response(self.replied_devices);
        self.completion.send(Ok(response));
    }
}

pub(crate) type RecoveryResultSender = oneshot::Sender<Result<u128, Error>>;

pub(crate) fn devices_sent_to(sent: Result<Sent, UdpError>) -> Result<u128, Error> {
    let sent = sent?;
    match sent.unsent {
        Some(unsent) => Err(UdpError::from(unsent).into()),
        None => Ok(sent.accepted_devices),
    }
}

pub(crate) enum SendOutcome {
    Failed(Option<InflightRequest>, Error),
    SentToNone(Option<InflightRequest>),
    SentToSome(Option<InflightRequest>),
}

pub(crate) struct SessionState {
    pub(crate) awaiting_reply: VecDeque<InflightRequest>,
    pub(crate) head_ack_clock_started_at: Instant,
    pub(crate) recovery_waiter: Option<RecoveryResultSender>,
    pub(crate) ack_timeout: Duration,
    pub(crate) network_failure: Option<NetworkCause>,
    pub(crate) is_closed: bool,
    pub(crate) driver_wakes_at: Option<Instant>,
}

impl SessionState {
    pub(crate) fn closed_error(&self) -> Option<Error> {
        if let Some(cause) = &self.network_failure {
            return Some(Error::Network(cause.clone()));
        }
        self.is_closed.then_some(Error::Closed)
    }

    pub(crate) fn push_awaiting(&mut self, request: InflightRequest) {
        if let Some(error) = self.closed_error() {
            request.completion.send(Err(error));
            return;
        }
        if self.awaiting_reply.is_empty() {
            self.head_ack_clock_started_at = request.sent_at;
        }
        self.awaiting_reply.push_back(request);
    }

    pub(crate) fn register_recovery_waiter(
        &mut self,
        waiter: RecoveryResultSender,
    ) -> Result<(), Error> {
        if let Some(error) = self.closed_error() {
            return Err(error);
        }
        self.recovery_waiter = Some(waiter);
        Ok(())
    }

    pub(crate) fn remove_by_msg_id(&mut self, msg_id: u16) -> Option<InflightRequest> {
        let index = self
            .awaiting_reply
            .iter()
            .position(|request| request.msg_id == msg_id)?;
        let request = self.awaiting_reply.remove(index);
        if index == 0 {
            self.head_ack_clock_started_at = Instant::now();
        }
        request
    }

    pub(crate) fn record_sent_to(&mut self, msg_id: u16, sent_to: u128) -> Option<InflightRequest> {
        let index = self
            .awaiting_reply
            .iter()
            .position(|request| request.msg_id == msg_id)?;
        self.awaiting_reply[index].sent_to = Some(sent_to);
        if self.awaiting_reply[index].has_all_replies() {
            return self.remove_by_msg_id(msg_id);
        }
        if index == 0 {
            self.head_ack_clock_started_at = Instant::now();
        }
        None
    }

    pub(crate) fn apply_send_result(
        &mut self,
        msg_id: u16,
        send_result: Result<u128, Error>,
    ) -> SendOutcome {
        match send_result {
            Err(error) => SendOutcome::Failed(self.remove_by_msg_id(msg_id), error),
            Ok(0) => SendOutcome::SentToNone(self.remove_by_msg_id(msg_id)),
            Ok(sent_to) => SendOutcome::SentToSome(self.record_sent_to(msg_id, sent_to)),
        }
    }

    pub(crate) fn ack_deadline_precedes_driver_wake(
        &self,
        msg_id: u16,
        ack_timeout: Duration,
    ) -> bool {
        if self.awaiting_reply.front().map(|head| head.msg_id) != Some(msg_id) {
            return false;
        }
        self.head_ack_deadline(ack_timeout).is_some_and(|deadline| {
            self.driver_wakes_at
                .is_none_or(|wakes_at| deadline < wakes_at)
        })
    }

    fn head_ack_deadline(&self, ack_timeout: Duration) -> Option<Instant> {
        self.awaiting_reply
            .front()
            .filter(|head| head.sent_to.is_some())
            .and_then(|_| self.head_ack_clock_started_at.checked_add(ack_timeout))
    }

    pub(crate) fn take_all_awaiting(&mut self) -> Vec<InflightRequest> {
        self.awaiting_reply.drain(..).collect()
    }
}

fn fail_all(requests: Vec<InflightRequest>, error: impl Fn() -> Error) {
    for request in requests {
        request.completion.send(Err(error()));
    }
}

pub(crate) struct SharedSession {
    state: Mutex<SessionState>,
    stop_requested: AtomicBool,
}

impl SharedSession {
    pub(crate) fn new(ack_timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(SessionState {
                awaiting_reply: VecDeque::new(),
                head_ack_clock_started_at: Instant::now(),
                recovery_waiter: None,
                ack_timeout,
                network_failure: None,
                is_closed: false,
                driver_wakes_at: None,
            }),
            stop_requested: AtomicBool::new(false),
        })
    }

    pub(crate) fn lock_state(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }
}

struct ReplyHandler {
    session: Arc<SharedSession>,
    stats: BusStats,
}

pub struct Driver {
    bus: Arc<UdpBus>,
    reply_handler: ReplyHandler,
    is_shut_down: bool,
}

impl Driver {
    pub(crate) fn new(bus: Arc<UdpBus>, session: Arc<SharedSession>) -> Self {
        Self {
            reply_handler: ReplyHandler {
                stats: bus.stats(),
                session,
            },
            bus,
            is_shut_down: false,
        }
    }

    pub fn run(mut self) -> Result<(), Error> {
        while let Some(deadline) = self.poll()? {
            self.wait(deadline)?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<Instant>, Error> {
        if self.is_shut_down {
            return self.shutdown_result().map(|()| None);
        }
        if self.reply_handler.session.is_stopping() {
            self.shut_down(None);
            return Ok(None);
        }
        match self.process_once() {
            Ok(deadline) => Ok(Some(deadline)),
            Err(cause) => {
                self.shut_down(Some(&cause));
                Err(Error::Network(cause))
            }
        }
    }

    pub fn wait(&mut self, deadline: Instant) -> Result<(), Error> {
        if self.is_shut_down {
            return self.shutdown_result();
        }
        self.bus.wait_readable(deadline).map_err(|e| {
            tracing::error!("waiting for a reply failed: {e}");
            let cause = NetworkCause::new(e);
            self.shut_down(Some(&cause));
            Error::Network(cause)
        })
    }

    fn shutdown_result(&self) -> Result<(), Error> {
        match &self.reply_handler.session.lock_state().network_failure {
            Some(cause) => Err(Error::Network(cause.clone())),
            None => Ok(()),
        }
    }

    fn shut_down(&mut self, network_failure: Option<&NetworkCause>) {
        self.is_shut_down = true;
        let (requests, recovery_waiter) = {
            let mut state = self.reply_handler.session.lock_state();
            state.network_failure = network_failure.cloned();
            state.is_closed = true;
            (state.take_all_awaiting(), state.recovery_waiter.take())
        };
        let error = || {
            network_failure
                .cloned()
                .map_or(Error::Closed, Error::Network)
        };
        if let Some(recovery_waiter) = recovery_waiter {
            let _ = recovery_waiter.send(Err(error()));
        }
        fail_all(requests, error);
        self.bus.close();
        tracing::debug!("driver stopped");
    }

    fn process_once(&mut self) -> Result<Instant, NetworkCause> {
        self.handle_available_replies()?;
        let now = Instant::now();
        if self.reply_handler.is_head_ack_overdue(now) {
            self.handle_available_replies()?;
        }
        self.reply_handler.fail_all_if_head_ack_overdue(now);
        if let Some(recovered_devices) = self.bus.take_finished_recovery(now) {
            self.reply_handler.finish_recovery(recovered_devices);
        }
        Ok(self.next_wake_deadline(now))
    }

    fn handle_available_replies(&mut self) -> Result<(), NetworkCause> {
        loop {
            match self.bus.recv(Instant::now()) {
                Ok(Some(reply)) => self.reply_handler.handle_reply(&reply),
                Ok(None) => return Ok(()),
                Err(e) => {
                    tracing::error!("receiving failed: {e}");
                    return Err(NetworkCause::new(e));
                }
            }
        }
    }

    fn next_wake_deadline(&self, now: Instant) -> Instant {
        let mut deadline = now + IDLE_WAIT;
        if let Some(due) = self.bus.recovery_deadline() {
            deadline = deadline.min(due);
        }
        if let Some(due) = self.bus.lost_deadline() {
            deadline = deadline.min(due);
        }
        let mut state = self.reply_handler.session.lock_state();
        let ack_timeout = state.ack_timeout;
        if let Some(expiry) = state.head_ack_deadline(ack_timeout) {
            deadline = deadline.min(expiry);
        }
        state.driver_wakes_at = Some(deadline);
        deadline
    }
}

impl ReplyHandler {
    fn is_head_ack_overdue(&self, now: Instant) -> bool {
        let state = self.session.lock_state();
        state
            .head_ack_deadline(state.ack_timeout)
            .is_some_and(|deadline| now >= deadline)
    }

    fn fail_all_if_head_ack_overdue(&mut self, now: Instant) {
        if !self.is_head_ack_overdue(now) {
            return;
        }
        let (requests, timeout) = {
            let mut state = self.session.lock_state();
            (state.take_all_awaiting(), state.ack_timeout)
        };
        tracing::warn!(
            pending = requests.len(),
            "the head frame got no reply within the ack timeout; failing the pending frames"
        );
        fail_all(requests, || Error::Timeout { timeout });
    }

    fn finish_recovery(&mut self, recovered_devices: u128) {
        if let Some(recovery_waiter) = self.session.lock_state().recovery_waiter.take() {
            let _ = recovery_waiter.send(Ok(recovered_devices));
        }
    }

    fn handle_reply(&mut self, reply: &Reply) {
        let reply_bit = 1u128 << reply.device;
        let mut state = self.session.lock_state();
        let Some(index) = state
            .awaiting_reply
            .iter()
            .position(|request| request.msg_id == reply.msg_id)
        else {
            tracing::trace!(msg_id = reply.msg_id, "dropping a reply nothing waits for");
            return;
        };
        let request = &mut state.awaiting_reply[index];
        if request.replied_devices & reply_bit != 0 {
            tracing::trace!(
                msg_id = reply.msg_id,
                device = reply.device,
                "dropping a repeated reply"
            );
            return;
        }
        request.reply_slot.record_reply(
            reply.device,
            reply.error_code,
            reply.sys_time,
            reply.data(),
        );
        request.replied_devices |= reply_bit;
        if !request.has_all_replies() {
            return;
        }
        let request = state
            .remove_by_msg_id(reply.msg_id)
            .expect("the request is awaiting a reply");
        drop(state);
        request.complete(&self.stats);
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        if !self.is_shut_down {
            self.shut_down(None);
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Driver {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(self.bus.socket())
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Driver {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        std::os::windows::io::AsSocket::as_socket(self.bus.socket())
    }
}

#[cfg(test)]
mod tests {
    use super::super::completion;
    use super::super::pool::ReplySlotPool;
    use super::*;

    fn shared_session() -> Arc<SharedSession> {
        SharedSession::new(Duration::from_millis(10))
    }

    fn inflight_request(
        msg_id: u16,
        replied_devices: u128,
    ) -> (InflightRequest, completion::ResponseFuture) {
        let (completion, rx) = completion::channel();
        let request = InflightRequest {
            msg_id,
            sent_to: None,
            replied_devices,
            sent_at: Instant::now(),
            reply_slot: pollster::block_on(ReplySlotPool::new(2).acquire()),
            completion,
        };
        (request, rx)
    }

    #[test]
    fn a_recovery_waits_until_the_driver_reports_it() {
        let session = shared_session();
        let (tx, mut rx) = oneshot::channel();
        session.lock_state().register_recovery_waiter(tx).unwrap();
        assert!(rx.try_recv().unwrap().is_none());

        let mut reply_handler = ReplyHandler {
            session: Arc::clone(&session),
            stats: BusStats::default(),
        };
        reply_handler.finish_recovery(0b10);

        assert_eq!(rx.try_recv().unwrap().unwrap().unwrap(), 0b10);
        assert!(session.lock_state().recovery_waiter.is_none());
    }

    #[test]
    fn a_recovery_is_refused_once_closed() {
        let session = shared_session();
        session.lock_state().is_closed = true;
        let (tx, _rx) = oneshot::channel();
        assert!(matches!(
            session.lock_state().register_recovery_waiter(tx),
            Err(Error::Closed)
        ));
        assert!(session.lock_state().recovery_waiter.is_none());
    }

    #[test]
    fn a_frame_answered_before_its_send_returns_is_handed_back_when_the_send_does() {
        let session = shared_session();
        let (request, _rx) = inflight_request(7, 0b11);
        session.lock_state().push_awaiting(request);

        let answered = session.lock_state().record_sent_to(7, 0b11);

        assert_eq!(answered.map(|request| request.msg_id), Some(7));
        assert!(session.lock_state().awaiting_reply.is_empty());
    }

    #[test]
    fn a_frame_still_missing_a_reply_stays_pending_after_its_send_returns() {
        let session = shared_session();
        let (request, _rx) = inflight_request(7, 0b01);
        session.lock_state().push_awaiting(request);

        assert!(session.lock_state().record_sent_to(7, 0b11).is_none());

        let state = session.lock_state();
        assert_eq!(state.awaiting_reply.len(), 1);
        assert_eq!(state.awaiting_reply[0].sent_to, Some(0b11));
    }

    #[test]
    fn the_head_asks_for_a_wake_only_when_its_ack_deadline_precedes_the_driver_wake() {
        const ACK_TIMEOUT: Duration = Duration::from_millis(10);
        let session = shared_session();
        let (request, _rx) = inflight_request(7, 0);
        session.lock_state().push_awaiting(request);
        assert!(
            !session
                .lock_state()
                .ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT)
        );

        assert!(session.lock_state().record_sent_to(7, 0b11).is_none());
        let deadline = session.lock_state().head_ack_deadline(ACK_TIMEOUT).unwrap();

        let mut state = session.lock_state();
        assert!(state.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        state.driver_wakes_at = Some(deadline + Duration::from_millis(1));
        assert!(state.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        state.driver_wakes_at = Some(deadline);
        assert!(!state.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        assert!(!state.ack_deadline_precedes_driver_wake(7, Duration::MAX));
    }

    #[test]
    fn a_frame_behind_the_head_never_asks_for_a_wake() {
        const ACK_TIMEOUT: Duration = Duration::from_millis(10);
        let session = shared_session();
        let (head, _head_rx) = inflight_request(1, 0);
        let (next, _next_rx) = inflight_request(2, 0);
        let mut state = session.lock_state();
        state.push_awaiting(head);
        state.push_awaiting(next);
        assert!(state.record_sent_to(1, 0b11).is_none());
        assert!(state.record_sent_to(2, 0b11).is_none());

        assert!(state.ack_deadline_precedes_driver_wake(1, ACK_TIMEOUT));
        assert!(!state.ack_deadline_precedes_driver_wake(2, ACK_TIMEOUT));
    }

    #[test]
    fn a_repeated_reply_from_a_device_keeps_the_first_one() {
        let session = shared_session();
        let (request, rx) = inflight_request(7, 0);
        session.lock_state().push_awaiting(request);
        assert!(session.lock_state().record_sent_to(7, 0b11).is_none());
        let mut reply_handler = ReplyHandler {
            session: Arc::clone(&session),
            stats: BusStats::default(),
        };

        reply_handler.handle_reply(&Reply::new(0, 7, 0, 0, &[]));
        reply_handler.handle_reply(&Reply::new(0, 7, 5, 0, &[]));
        reply_handler.handle_reply(&Reply::new(1, 7, 0, 0, &[]));

        assert_eq!(pollster::block_on(rx).unwrap().error_codes(), [0, 0]);
    }

    #[test]
    fn a_frame_is_never_answered_while_its_send_is_still_running() {
        let (request, _rx) = inflight_request(7, u128::MAX);
        assert!(!request.has_all_replies());
    }
}

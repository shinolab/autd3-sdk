use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_rs_core::BusStats;
use futures_channel::oneshot;

use crate::error::{Error, NetworkCause};
use crate::protocol::Seq;
use crate::udp::{Reply, Sent, UdpBus, UdpError};

use super::completion::CompletionSender;
use super::config::ClientConfig;
use super::pool::Slot;

const RESET_ACK: u8 = 0xFF;
const IDLE_WAIT: Duration = Duration::from_secs(1);

pub(crate) struct Inflight {
    pub(crate) msg_id: u16,
    pub(crate) seq: Option<Seq>,
    pub(crate) expected: Option<u128>,
    pub(crate) replied: u128,
    pub(crate) sent_at: Instant,
    pub(crate) slot: Option<Slot>,
    pub(crate) tx: CompletionSender,
}

impl Inflight {
    fn is_answered(&self) -> bool {
        self.expected
            .is_some_and(|expected| self.replied & expected == expected)
    }

    pub(crate) fn complete(self, stats: &BusStats) {
        tracing::trace!(msg_id = self.msg_id, "frame answered by every device");
        stats.record_ack(u64::try_from(self.sent_at.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let response = match &self.slot {
            Some(slot) => slot.response(self.replied),
            None => crate::response::Response::from_status(&[]),
        };
        self.tx.send(Ok(response));
    }
}

pub(crate) type RecoverySender = oneshot::Sender<Result<u128, Error>>;

pub(crate) struct SendFailure {
    pub(crate) error: Error,
    pub(crate) unsent: bool,
}

impl From<UdpError> for SendFailure {
    fn from(e: UdpError) -> Self {
        Self {
            unsent: matches!(e, UdpError::Unsent { .. }),
            error: e.into(),
        }
    }
}

pub(crate) fn sent_devices(sent: Result<Sent, UdpError>) -> Result<u128, SendFailure> {
    let sent = sent?;
    match sent.unsent {
        Some(unsent) => Err(UdpError::from(unsent).into()),
        None => Ok(sent.devices),
    }
}

pub(crate) enum Settlement {
    Failed(Option<Inflight>, Error),
    Unreached(Option<Inflight>),
    Reached(Option<Inflight>),
}

pub(crate) struct Pending {
    pub(crate) entries: VecDeque<Inflight>,
    pub(crate) head_since: Instant,
    pub(crate) need_reset: bool,
    pub(crate) recovery: Option<RecoverySender>,
    pub(crate) config: ClientConfig,
    pub(crate) next_seq: Seq,
    pub(crate) failure: Option<NetworkCause>,
    pub(crate) closed: bool,
    pub(crate) driver_wakes_at: Option<Instant>,
}

impl Pending {
    pub(crate) fn closed_error(&self) -> Option<Error> {
        if let Some(cause) = &self.failure {
            return Some(Error::Network(cause.clone()));
        }
        self.closed.then_some(Error::Closed)
    }

    pub(crate) fn push(&mut self, entry: Inflight) {
        if let Some(error) = self.closed_error() {
            entry.tx.send(Err(error));
            return;
        }
        if self.entries.is_empty() {
            self.head_since = entry.sent_at;
        }
        self.entries.push_back(entry);
    }

    pub(crate) fn await_recovery(&mut self, tx: RecoverySender) -> Result<(), Error> {
        if let Some(error) = self.closed_error() {
            return Err(error);
        }
        self.recovery = Some(tx);
        Ok(())
    }

    pub(crate) fn withdraw(&mut self, msg_id: u16) -> Option<Inflight> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.msg_id == msg_id)?;
        let entry = self.entries.remove(index);
        if index == 0 {
            self.head_since = Instant::now();
        }
        entry
    }

    pub(crate) fn sent(&mut self, msg_id: u16, devices: u128) -> Option<Inflight> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.msg_id == msg_id)?;
        self.entries[index].expected = Some(devices);
        if self.entries[index].is_answered() {
            return self.withdraw(msg_id);
        }
        if index == 0 {
            self.head_since = Instant::now();
        }
        None
    }

    pub(crate) fn settle(&mut self, msg_id: u16, sent: Result<u128, SendFailure>) -> Settlement {
        match sent {
            Err(failure) => {
                if failure.unsent {
                    self.need_reset = true;
                }
                Settlement::Failed(self.withdraw(msg_id), failure.error)
            }
            Ok(0) => Settlement::Unreached(self.withdraw(msg_id)),
            Ok(devices) => Settlement::Reached(self.sent(msg_id, devices)),
        }
    }

    pub(crate) fn ack_deadline_precedes_driver_wake(
        &self,
        msg_id: u16,
        ack_timeout: Duration,
    ) -> bool {
        if self.entries.front().map(|head| head.msg_id) != Some(msg_id) {
            return false;
        }
        self.ack_deadline(ack_timeout).is_some_and(|deadline| {
            self.driver_wakes_at
                .is_none_or(|wakes_at| deadline < wakes_at)
        })
    }

    fn ack_deadline(&self, ack_timeout: Duration) -> Option<Instant> {
        self.entries
            .front()
            .filter(|head| head.expected.is_some())
            .and_then(|_| self.head_since.checked_add(ack_timeout))
    }

    pub(crate) fn reconfigure(&mut self, config: ClientConfig) {
        self.config = config;
    }

    pub(crate) fn take_all(&mut self) -> Vec<Inflight> {
        self.entries.drain(..).collect()
    }
}

fn fail(entries: Vec<Inflight>, error: impl Fn() -> Error) {
    for entry in entries {
        entry.tx.send(Err(error()));
    }
}

pub(crate) struct Shared {
    pending: Mutex<Pending>,
    stop: AtomicBool,
}

impl Shared {
    pub(crate) fn new(config: ClientConfig) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(Pending {
                entries: VecDeque::new(),
                head_since: Instant::now(),
                need_reset: true,
                recovery: None,
                config,
                next_seq: Seq::ZERO,
                failure: None,
                closed: false,
                driver_wakes_at: None,
            }),
            stop: AtomicBool::new(false),
        })
    }

    pub(crate) fn pending(&self) -> MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}

struct Session {
    shared: Arc<Shared>,
    stats: BusStats,
}

pub struct Driver {
    bus: Arc<UdpBus>,
    session: Session,
    finished: bool,
}

impl Driver {
    pub(crate) fn new(bus: Arc<UdpBus>, shared: Arc<Shared>) -> Self {
        Self {
            session: Session {
                stats: bus.stats(),
                shared,
            },
            bus,
            finished: false,
        }
    }

    pub fn run(mut self) -> Result<(), Error> {
        while let Some(deadline) = self.poll()? {
            self.wait(deadline)?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<Instant>, Error> {
        if self.finished {
            return self.outcome().map(|()| None);
        }
        if self.session.shared.is_stopping() {
            self.finish(None);
            return Ok(None);
        }
        match self.step() {
            Ok(deadline) => Ok(Some(deadline)),
            Err(cause) => {
                self.finish(Some(&cause));
                Err(Error::Network(cause))
            }
        }
    }

    pub fn wait(&mut self, deadline: Instant) -> Result<(), Error> {
        if self.finished {
            return self.outcome();
        }
        self.bus.wait_readable(deadline).map_err(|e| {
            tracing::error!("waiting for a reply failed: {e}");
            let cause = NetworkCause::new(e);
            self.finish(Some(&cause));
            Error::Network(cause)
        })
    }

    fn outcome(&self) -> Result<(), Error> {
        match &self.session.shared.pending().failure {
            Some(cause) => Err(Error::Network(cause.clone())),
            None => Ok(()),
        }
    }

    fn finish(&mut self, outcome: Option<&NetworkCause>) {
        self.finished = true;
        let (entries, recovery) = {
            let mut pending = self.session.shared.pending();
            pending.failure = outcome.cloned();
            pending.closed = true;
            (pending.take_all(), pending.recovery.take())
        };
        let error = || outcome.cloned().map_or(Error::Closed, Error::Network);
        if let Some(recovery) = recovery {
            let _ = recovery.send(Err(error()));
        }
        fail(entries, error);
        let _ = self.bus.close();
        tracing::debug!("driver stopped");
    }

    fn step(&mut self) -> Result<Instant, NetworkCause> {
        self.drain()?;
        let now = Instant::now();
        if self.session.ack_overdue(now) {
            self.drain()?;
        }
        self.session.check_timeouts(now);
        if let Some(recovered) = self.bus.settle_recovery(now) {
            self.session.recovered(recovered);
        }
        Ok(self.next_deadline(now))
    }

    fn drain(&mut self) -> Result<(), NetworkCause> {
        loop {
            match self.bus.recv(Instant::now()) {
                Ok(Some(reply)) => self.session.dispatch(&reply),
                Ok(None) => return Ok(()),
                Err(e) => {
                    tracing::error!("receiving failed: {e}");
                    return Err(NetworkCause::new(e));
                }
            }
        }
    }

    fn next_deadline(&self, now: Instant) -> Instant {
        let mut deadline = now + IDLE_WAIT;
        if let Some(due) = self.bus.recovery_deadline() {
            deadline = deadline.min(due);
        }
        if let Some(due) = self.bus.lost_deadline() {
            deadline = deadline.min(due);
        }
        let mut pending = self.session.shared.pending();
        let ack_timeout = pending.config.ack_timeout;
        if let Some(expiry) = pending.ack_deadline(ack_timeout) {
            deadline = deadline.min(expiry);
        }
        pending.driver_wakes_at = Some(deadline);
        deadline
    }
}

impl Session {
    fn ack_overdue(&self, now: Instant) -> bool {
        let pending = self.shared.pending();
        pending
            .ack_deadline(pending.config.ack_timeout)
            .is_some_and(|deadline| now >= deadline)
    }

    fn check_timeouts(&mut self, now: Instant) {
        if !self.ack_overdue(now) {
            return;
        }
        let (entries, timeout) = {
            let mut pending = self.shared.pending();
            pending.need_reset = true;
            (pending.take_all(), pending.config.ack_timeout)
        };
        tracing::warn!(
            pending = entries.len(),
            "the head frame got no reply within the ack timeout; failing the pending frames"
        );
        fail(entries, || Error::Timeout { timeout });
    }

    fn recovered(&mut self, recovered: u128) {
        let mut pending = self.shared.pending();
        if recovered != 0 {
            pending.need_reset = true;
        }
        if let Some(recovery) = pending.recovery.take() {
            let _ = recovery.send(Ok(recovered));
        }
    }

    fn dispatch(&mut self, reply: &Reply) {
        let bit = 1u128 << reply.device;
        let mut pending = self.shared.pending();
        let Some(index) = pending
            .entries
            .iter()
            .position(|entry| entry.msg_id == reply.msg_id)
        else {
            tracing::trace!(msg_id = reply.msg_id, "dropping a reply nothing waits for");
            return;
        };
        let entry = &mut pending.entries[index];
        match entry.seq {
            None => {
                if reply.ack != RESET_ACK {
                    return;
                }
            }
            Some(seq) => {
                if reply.ack != seq.get() {
                    let entry = pending.entries.remove(index).expect("index is in range");
                    pending.need_reset = true;
                    if index == 0 {
                        pending.head_since = Instant::now();
                    }
                    drop(pending);
                    tracing::warn!(
                        device = reply.device,
                        expected = seq.get(),
                        got = reply.ack,
                        "the device did not accept the frame; a reset is required"
                    );
                    entry.tx.send(Err(Error::SeqMismatch {
                        device: reply.device,
                        expected: seq.get(),
                        got: reply.ack,
                    }));
                    return;
                }
                if let Some(slot) = &mut entry.slot {
                    slot.record_reply(reply.device, reply.status, reply.data());
                }
            }
        }
        entry.replied |= bit;
        if !entry.is_answered() {
            return;
        }
        let entry = pending
            .withdraw(reply.msg_id)
            .expect("the entry is pending");
        drop(pending);
        entry.complete(&self.stats);
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(None);
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Driver {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&*self.bus)
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Driver {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        std::os::windows::io::AsSocket::as_socket(&*self.bus)
    }
}

#[cfg(test)]
mod tests {
    use super::super::completion;
    use super::*;

    fn pending() -> Arc<Shared> {
        Shared::new(ClientConfig::default())
    }

    fn inflight(msg_id: u16, replied: u128) -> (Inflight, completion::ResponseFuture) {
        let (tx, rx) = completion::channel();
        let entry = Inflight {
            msg_id,
            seq: Some(Seq::ZERO),
            expected: None,
            replied,
            sent_at: Instant::now(),
            slot: None,
            tx,
        };
        (entry, rx)
    }

    #[test]
    fn a_recovery_waits_until_the_driver_reports_it() {
        let shared = pending();
        let (tx, mut rx) = oneshot::channel();
        shared.pending().await_recovery(tx).unwrap();
        assert!(rx.try_recv().unwrap().is_none());

        let mut session = Session {
            shared: Arc::clone(&shared),
            stats: BusStats::default(),
        };
        shared.pending().need_reset = false;
        session.recovered(0b10);

        assert_eq!(rx.try_recv().unwrap().unwrap().unwrap(), 0b10);
        assert!(shared.pending().need_reset);
    }

    #[test]
    fn a_recovery_bringing_nothing_back_needs_no_reset() {
        let shared = pending();
        let (tx, mut rx) = oneshot::channel();
        shared.pending().await_recovery(tx).unwrap();
        shared.pending().need_reset = false;

        Session {
            shared: Arc::clone(&shared),
            stats: BusStats::default(),
        }
        .recovered(0);

        assert_eq!(rx.try_recv().unwrap().unwrap().unwrap(), 0);
        assert!(!shared.pending().need_reset);
    }

    #[test]
    fn a_recovery_reported_with_nobody_waiting_still_needs_a_reset() {
        let shared = pending();
        shared.pending().need_reset = false;

        Session {
            shared: Arc::clone(&shared),
            stats: BusStats::default(),
        }
        .recovered(0b1);

        assert!(shared.pending().need_reset);
        assert!(shared.pending().recovery.is_none());
    }

    #[test]
    fn a_recovery_is_refused_once_closed() {
        let shared = pending();
        shared.pending().closed = true;
        let (tx, _rx) = oneshot::channel();
        assert!(matches!(
            shared.pending().await_recovery(tx),
            Err(Error::Closed)
        ));
        assert!(shared.pending().recovery.is_none());
    }

    #[test]
    fn a_frame_answered_before_its_send_returns_is_handed_back_when_the_send_does() {
        let shared = pending();
        let (entry, _rx) = inflight(7, 0b11);
        shared.pending().push(entry);

        let answered = shared.pending().sent(7, 0b11);

        assert_eq!(answered.map(|entry| entry.msg_id), Some(7));
        assert!(shared.pending().entries.is_empty());
    }

    #[test]
    fn a_frame_still_missing_a_reply_stays_pending_after_its_send_returns() {
        let shared = pending();
        let (entry, _rx) = inflight(7, 0b01);
        shared.pending().push(entry);

        assert!(shared.pending().sent(7, 0b11).is_none());

        let pending = shared.pending();
        assert_eq!(pending.entries.len(), 1);
        assert_eq!(pending.entries[0].expected, Some(0b11));
    }

    #[test]
    fn the_head_asks_for_a_wake_only_when_its_ack_deadline_precedes_the_driver_wake() {
        const ACK_TIMEOUT: Duration = Duration::from_millis(10);
        let shared = pending();
        let (entry, _rx) = inflight(7, 0);
        shared.pending().push(entry);
        assert!(
            !shared
                .pending()
                .ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT)
        );

        assert!(shared.pending().sent(7, 0b11).is_none());
        let deadline = shared.pending().ack_deadline(ACK_TIMEOUT).unwrap();

        let mut pending = shared.pending();
        assert!(pending.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        pending.driver_wakes_at = Some(deadline + Duration::from_millis(1));
        assert!(pending.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        pending.driver_wakes_at = Some(deadline);
        assert!(!pending.ack_deadline_precedes_driver_wake(7, ACK_TIMEOUT));
        assert!(!pending.ack_deadline_precedes_driver_wake(7, Duration::MAX));
    }

    #[test]
    fn a_frame_behind_the_head_never_asks_for_a_wake() {
        const ACK_TIMEOUT: Duration = Duration::from_millis(10);
        let shared = pending();
        let (head, _head_rx) = inflight(1, 0);
        let (next, _next_rx) = inflight(2, 0);
        let mut pending = shared.pending();
        pending.push(head);
        pending.push(next);
        assert!(pending.sent(1, 0b11).is_none());
        assert!(pending.sent(2, 0b11).is_none());

        assert!(pending.ack_deadline_precedes_driver_wake(1, ACK_TIMEOUT));
        assert!(!pending.ack_deadline_precedes_driver_wake(2, ACK_TIMEOUT));
    }

    #[test]
    fn a_frame_is_never_answered_while_its_send_is_still_running() {
        let (entry, _rx) = inflight(7, u128::MAX);
        assert!(!entry.is_answered());
    }
}

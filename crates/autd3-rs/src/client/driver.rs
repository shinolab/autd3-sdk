use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_rs_core::BusStats;

use crate::error::{Error, NetworkCause};
use crate::link::LinkError;
use crate::link::message::{Notice, Outcome};
use crate::protocol::Seq;
use crate::udp::{Reply, Sent, UdpError};
use crate::value::SysTime;

use super::completion::CompletionSender;
use super::config::ClientConfig;
use super::pool::Slot;
use super::transport::{Port, Transport};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Restoring {
    pub(crate) devices: u128,
    failures: u32,
}

impl Restoring {
    pub(crate) fn recovered(&mut self, devices: u128) {
        self.devices |= devices;
        self.failures = 0;
    }

    pub(crate) fn restored(&mut self, devices: u128) {
        self.devices &= !devices;
        self.failures = 0;
    }

    pub(crate) fn failed(&mut self, devices: u128, attempts: NonZeroU32) -> bool {
        self.failures += 1;
        if self.failures < attempts.get() {
            return false;
        }
        self.devices &= !devices;
        self.failures = 0;
        true
    }
}

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
    pub(crate) resync: Restoring,
    pub(crate) release_failsafe: Restoring,
    pub(crate) recoveries: u64,
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
        if config.resynchronize_on_recovery.is_none() {
            self.resync = Restoring::default();
        }
        if config.release_failsafe_on_recovery.is_none() {
            self.release_failsafe = Restoring::default();
        }
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
                resync: Restoring::default(),
                release_failsafe: Restoring::default(),
                recoveries: 0,
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
    transport: Arc<Transport>,
    port: Port,
    session: Session,
    maintenance_due: Option<Instant>,
    finished: bool,
}

impl Driver {
    pub(crate) fn new(transport: Arc<Transport>, port: Port, shared: Arc<Shared>) -> Self {
        Self {
            session: Session {
                stats: transport.stats(),
                shared,
            },
            transport,
            port,
            maintenance_due: None,
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
        self.port.wait_readable(deadline).map_err(|e| {
            tracing::error!("waiting for a reply failed: {e}");
            let cause = match e {
                Error::Network(cause) => cause,
                e => NetworkCause::new(e),
            };
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
        let entries = {
            let mut pending = self.session.shared.pending();
            pending.failure = outcome.cloned();
            pending.closed = true;
            pending.take_all()
        };
        fail(entries, || {
            outcome.cloned().map_or(Error::Closed, Error::Network)
        });
        self.transport.close();
        tracing::debug!("driver stopped");
    }

    fn step(&mut self) -> Result<Instant, NetworkCause> {
        self.drain()?;
        let now = Instant::now();
        if self.session.ack_overdue(now) {
            self.drain()?;
        }
        self.session.check_timeouts(now);
        self.maintain(now)?;
        Ok(self.next_deadline(now))
    }

    fn drain(&mut self) -> Result<(), NetworkCause> {
        match &mut self.port {
            Port::Udp(pump) => loop {
                match pump.recv() {
                    Ok(Some(reply)) => self.session.dispatch(&reply),
                    Ok(None) => return Ok(()),
                    Err(e) => {
                        tracing::error!("receiving failed: {e}");
                        return Err(NetworkCause::new(e));
                    }
                }
            },
            Port::Server(reader) => {
                let session = &mut self.session;
                let transport = &*self.transport;
                reader
                    .drain(|notice| session.notice(transport, notice))
                    .map_err(|e| {
                        tracing::error!("receiving from the server failed: {e}");
                        NetworkCause::new(e)
                    })
            }
        }
    }

    fn maintain(&mut self, now: Instant) -> Result<(), NetworkCause> {
        if let Port::Udp(pump) = &mut self.port {
            self.maintenance_due = pump.maintain(now, true).map_err(NetworkCause::new)?;
            let recovered = pump.take_recovered();
            self.session.recovered(recovered);
        }
        Ok(())
    }

    fn next_deadline(&self, now: Instant) -> Instant {
        let mut deadline = now + IDLE_WAIT;
        if let Some(due) = self.maintenance_due {
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
        if recovered == 0 {
            return;
        }
        let mut pending = self.shared.pending();
        pending.need_reset = true;
        pending.recoveries += 1;
        if pending.config.resynchronize_on_recovery.is_some() {
            pending.resync.recovered(recovered);
        }
        if pending.config.release_failsafe_on_recovery.is_some() {
            pending.release_failsafe.recovered(recovered);
        }
    }

    fn notice(&mut self, transport: &Transport, notice: Notice<'_>) -> Result<(), LinkError> {
        let Transport::Server(conn) = transport else {
            return Ok(());
        };
        match notice {
            Notice::Reply {
                msg_id,
                device,
                ack,
                status,
                data,
            } => self.dispatch(&Reply::new(usize::from(device), msg_id, ack, status, data)),
            Notice::Settled { msg_id, outcome } => self.settle_deferred(msg_id, outcome),
            Notice::States(states) => {
                for (device, state) in states.into_iter().enumerate() {
                    conn.state().publish(device, state);
                }
            }
            Notice::Recovered(recovered) => {
                for _ in 0..recovered.count_ones() {
                    self.stats.record_recovery();
                }
                self.recovered(recovered);
            }
            Notice::Clock(sys_time) => {
                if let Some(sys_time) = NonZeroU64::new(sys_time) {
                    conn.device_clock()
                        .observe(SysTime::from_nanos(sys_time.get()));
                }
            }
            Notice::Wake => {}
            Notice::Failure(reason) => return Err(LinkError::ServerFailed(reason.to_owned())),
            Notice::Welcome { .. } | Notice::Reject { .. } => {
                return Err(LinkError::Unexpected(
                    "a handshake answer after the handshake",
                ));
            }
        }
        Ok(())
    }

    fn settle_deferred(&mut self, msg_id: u16, outcome: Outcome<'_>) {
        let sent = match outcome {
            Outcome::Sent { devices } => Ok(devices),
            Outcome::Unsent { device, reason, .. } => Err(SendFailure::from(UdpError::Unsent {
                device: usize::from(device),
                source: std::io::Error::other(reason.to_owned()),
            })),
            Outcome::Failed { reason } => Err(SendFailure {
                error: Error::Network(NetworkCause::new(LinkError::SendFailed(reason.to_owned()))),
                unsent: false,
            }),
        };
        let (is_frame, settlement, timeout) = {
            let mut pending = self.shared.pending();
            let is_frame = pending
                .entries
                .iter()
                .find(|entry| entry.msg_id == msg_id)
                .is_some_and(|entry| entry.seq.is_some());
            let settlement = pending.settle(msg_id, sent);
            (is_frame, settlement, pending.config.ack_timeout)
        };
        match settlement {
            Settlement::Failed(entry, error) => {
                if let Some(entry) = entry {
                    entry.tx.send(Err(error));
                }
            }
            Settlement::Unreached(entry) => {
                if let Some(entry) = entry {
                    entry.tx.send(Err(Error::Timeout { timeout }));
                }
            }
            Settlement::Reached(answered) => {
                if is_frame {
                    self.stats.record_frame();
                }
                if let Some(entry) = answered {
                    entry.complete(&self.stats);
                }
            }
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
        std::os::fd::AsFd::as_fd(&self.port)
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Driver {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        std::os::windows::io::AsSocket::as_socket(&self.port)
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
    fn a_restoration_is_given_up_after_the_allowed_failures() {
        let attempts = NonZeroU32::new(3).unwrap();
        let mut restoring = Restoring::default();
        restoring.recovered(0b110);
        assert!(!restoring.failed(0b010, attempts));
        assert!(!restoring.failed(0b010, attempts));
        assert_eq!(restoring.devices, 0b110);
        assert!(restoring.failed(0b010, attempts));
        assert_eq!(restoring.devices, 0b100);
        assert!(!restoring.failed(0b100, attempts));
    }

    #[test]
    fn a_success_or_a_new_recovery_restarts_the_failure_count() {
        let attempts = NonZeroU32::new(2).unwrap();
        let mut restoring = Restoring::default();
        restoring.recovered(0b11);
        assert!(!restoring.failed(0b11, attempts));
        restoring.restored(0b01);
        assert_eq!(restoring.devices, 0b10);
        assert!(!restoring.failed(0b10, attempts));
        restoring.recovered(0b100);
        assert!(!restoring.failed(0b110, attempts));
        assert!(restoring.failed(0b110, attempts));
        assert_eq!(restoring.devices, 0);
    }

    #[test]
    fn a_single_allowed_attempt_gives_up_at_the_first_failure() {
        let mut restoring = Restoring::default();
        restoring.recovered(0b1);
        assert!(restoring.failed(0b1, NonZeroU32::MIN));
        assert_eq!(restoring.devices, 0);
    }

    #[test]
    fn disabling_a_restoration_drops_the_devices_waiting_for_it() {
        let shared = pending();
        let mut pending = shared.pending();
        pending.resync.recovered(0b01);
        pending.release_failsafe.recovered(0b10);

        pending.reconfigure(ClientConfig {
            resynchronize_on_recovery: None,
            ..ClientConfig::default()
        });
        assert_eq!(pending.resync.devices, 0);
        assert_eq!(pending.release_failsafe.devices, 0b10);
        assert_eq!(pending.config.resynchronize_on_recovery, None);

        pending.reconfigure(ClientConfig {
            release_failsafe_on_recovery: None,
            ..ClientConfig::default()
        });
        assert_eq!(pending.release_failsafe.devices, 0);
    }

    #[test]
    fn changing_only_the_attempts_keeps_the_devices_waiting() {
        let shared = pending();
        let mut pending = shared.pending();
        pending.resync.recovered(0b01);
        pending.release_failsafe.recovered(0b10);

        pending.reconfigure(ClientConfig {
            resynchronize_on_recovery: NonZeroU32::new(1),
            release_failsafe_on_recovery: NonZeroU32::new(5),
            ..ClientConfig::default()
        });

        assert_eq!(pending.resync.devices, 0b01);
        assert_eq!(pending.release_failsafe.devices, 0b10);
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

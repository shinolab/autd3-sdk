use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::{DeviceState, DeviceStatus};

use super::error::UdpError;

const STATE_READY: u8 = 0;
const STATE_SYNCING: u8 = 1;
const STATE_LOST: u8 = 2;
pub(super) const SYNCED: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tracker {
    unanswered_since: Option<Instant>,
    send_failing_since: Option<Instant>,
    state: DeviceState,
}

impl Tracker {
    pub(crate) const fn new() -> Self {
        Self {
            unanswered_since: None,
            send_failing_since: None,
            state: DeviceState::Ready,
        }
    }

    pub(crate) fn send_failed(&mut self, now: Instant, lost_timeout: Duration) {
        let since = *self.send_failing_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= lost_timeout {
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn requested(&mut self, now: Instant) {
        self.unanswered_since.get_or_insert(now);
    }

    pub(crate) fn unreached(&mut self, requested_at: Instant) {
        if self.unanswered_since == Some(requested_at) {
            self.unanswered_since = None;
        }
    }

    pub(crate) fn replied(&mut self, unit: u8, unit_id: u8, flags: Flags) -> bool {
        if self.is_lost() {
            return false;
        }
        if !flags.contains(Flags::ASSIGNED) || unit_id != unit {
            self.state = DeviceState::Lost;
            return false;
        }
        self.unanswered_since = None;
        self.send_failing_since = None;
        self.state = if flags.contains(SYNCED) {
            DeviceState::Ready
        } else {
            DeviceState::Syncing
        };
        true
    }

    pub(crate) fn expire(&mut self, now: Instant, lost_timeout: Duration) {
        if self
            .unanswered_since
            .is_some_and(|since| now.saturating_duration_since(since) >= lost_timeout)
        {
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn state(self) -> DeviceState {
        self.state
    }

    pub(crate) fn is_lost(self) -> bool {
        self.state == DeviceState::Lost
    }
}

#[derive(Debug)]
pub(crate) struct SharedState {
    states: Box<[AtomicU8]>,
    closed: AtomicBool,
}

impl SharedState {
    pub(crate) fn new(num_devices: usize) -> Arc<Self> {
        Arc::new(Self {
            states: (0..num_devices)
                .map(|_| AtomicU8::new(STATE_READY))
                .collect(),
            closed: AtomicBool::new(false),
        })
    }

    pub(crate) fn publish(&self, device: usize, state: DeviceState) {
        let encoded = match state {
            DeviceState::Ready => STATE_READY,
            DeviceState::Lost => STATE_LOST,
            _ => STATE_SYNCING,
        };
        self.states[device].store(encoded, Ordering::Relaxed);
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    fn snapshot(&self) -> Vec<DeviceState> {
        self.states
            .iter()
            .map(|s| match s.load(Ordering::Relaxed) {
                STATE_READY => DeviceState::Ready,
                STATE_LOST => DeviceState::Lost,
                _ => DeviceState::Syncing,
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct StateChecker {
    shared: Arc<SharedState>,
}

impl StateChecker {
    pub(crate) fn new(shared: Arc<SharedState>) -> Self {
        Self { shared }
    }

    pub fn check(&self) -> Result<DeviceStatus, UdpError> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(UdpError::Closed);
        }
        Ok(DeviceStatus::new(self.shared.snapshot()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Flags = Flags::ASSIGNED
        .union(Flags::PTP_LOCKED)
        .union(Flags::SYNC_READY);
    const LOST_AFTER: Duration = Duration::from_millis(100);

    #[test]
    fn a_request_that_never_left_does_not_count_towards_lost() {
        let mut t = Tracker::new();
        let now = Instant::now();
        t.requested(now);
        t.unreached(now);
        t.expire(now + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn an_earlier_unanswered_request_outlives_a_later_one_that_never_left() {
        let mut t = Tracker::new();
        let earlier = Instant::now();
        let later = earlier + Duration::from_millis(1);
        t.requested(earlier);
        t.requested(later);
        t.unreached(later);
        t.expire(earlier + LOST_AFTER, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Lost);
    }

    #[test]
    fn a_send_that_fails_once_does_not_make_a_device_lost() {
        let mut t = Tracker::new();
        let now = Instant::now();
        t.send_failed(now, LOST_AFTER);
        t.expire(now + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn sends_that_keep_failing_past_the_lost_timeout_make_a_device_lost() {
        let mut t = Tracker::new();
        let start = Instant::now();
        t.send_failed(start, LOST_AFTER);
        t.send_failed(start + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
        t.send_failed(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Lost);
    }

    #[test]
    fn a_reply_between_two_failed_sends_restarts_the_count() {
        let mut t = Tracker::new();
        let start = Instant::now();
        t.send_failed(start, LOST_AFTER);
        assert!(t.replied(1, 1, READY));
        t.send_failed(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
        t.send_failed(start + LOST_AFTER * 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Lost);
    }

    #[test]
    fn a_ready_reply_is_ready() {
        let mut t = Tracker::new();
        assert!(t.replied(2, 2, READY));
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_pulse_is_syncing() {
        let mut t = Tracker::new();
        assert!(t.replied(0, 0, Flags::ASSIGNED));
        assert_eq!(t.state(), DeviceState::Syncing);
        assert!(t.replied(0, 0, READY));
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_ptp_lock_is_syncing() {
        let mut t = Tracker::new();
        assert!(t.replied(1, 1, Flags::ASSIGNED | Flags::SYNC_READY));
        assert_eq!(t.state(), DeviceState::Syncing);
        assert!(t.replied(1, 1, Flags::ASSIGNED | Flags::PTP_LOCKED));
        assert_eq!(t.state(), DeviceState::Syncing);
        assert!(t.replied(1, 1, READY));
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unanswered_request_past_the_lost_timeout_makes_a_device_lost_for_good() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.requested(start);
        t.expire(start + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
        assert!(t.replied(1, 1, READY));
        let later = start + LOST_AFTER / 2;
        t.requested(later);
        t.requested(later + LOST_AFTER / 2);
        t.expire(start + LOST_AFTER, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
        t.expire(later + LOST_AFTER, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Lost);
        assert!(!t.replied(1, 1, READY));
        assert_eq!(t.state(), DeviceState::Lost);
    }

    #[test]
    fn silence_without_a_request_is_not_lost() {
        let start = Instant::now();
        let mut t = Tracker::new();
        assert!(t.replied(1, 1, READY));
        t.expire(start + LOST_AFTER * 10, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
        t.requested(start + LOST_AFTER * 10);
        t.expire(start + LOST_AFTER * 10 + LOST_AFTER / 2, LOST_AFTER);
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn an_unassigned_or_foreign_reply_is_lost() {
        let mut t = Tracker::new();
        assert!(!t.replied(1, 1, Flags::SYNC_READY));
        assert_eq!(t.state(), DeviceState::Lost);

        let mut t = Tracker::new();
        assert!(!t.replied(1, 3, READY));
        assert_eq!(t.state(), DeviceState::Lost);
    }

    #[test]
    fn the_checker_reads_the_published_states() {
        let shared = SharedState::new(3);
        shared.publish(1, DeviceState::Syncing);
        shared.publish(2, DeviceState::Lost);
        let checker = StateChecker::new(Arc::clone(&shared));
        let status = checker.check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Lost]
        );
        shared.close();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
    }
}

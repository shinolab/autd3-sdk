use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{FLAG_ASSIGNED, FLAG_PTP_LOCKED, FLAG_SYNC_READY};
use autd3_rs_core::{DeviceState, DeviceStatus};

use super::error::UdpError;

const STATE_READY: u8 = 0;
const STATE_SYNCING: u8 = 1;
const STATE_LOST: u8 = 2;
const SYNCED: u8 = FLAG_PTP_LOCKED | FLAG_SYNC_READY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tracker {
    unanswered_since: Option<Instant>,
    lost: bool,
    state: DeviceState,
}

impl Tracker {
    pub(crate) const fn new() -> Self {
        Self {
            unanswered_since: None,
            lost: false,
            state: DeviceState::Ready,
        }
    }

    pub(crate) fn requested(&mut self, now: Instant) {
        self.unanswered_since.get_or_insert(now);
    }

    pub(crate) fn replied(&mut self, unit: u8, unit_id: u8, flags: u8) -> bool {
        if self.lost {
            return false;
        }
        if flags & FLAG_ASSIGNED == 0 || unit_id != unit {
            self.lost = true;
            self.state = DeviceState::Lost;
            return false;
        }
        self.unanswered_since = None;
        self.state = if flags & SYNCED == SYNCED {
            DeviceState::Ready
        } else {
            DeviceState::Syncing
        };
        true
    }

    pub(crate) fn expire(&mut self, now: Instant, lost_timeout: Duration) {
        if !self.lost
            && self
                .unanswered_since
                .is_some_and(|since| now.saturating_duration_since(since) >= lost_timeout)
        {
            self.lost = true;
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn state(self) -> DeviceState {
        self.state
    }

    pub(crate) const fn is_lost(self) -> bool {
        self.lost
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

pub struct StateChecker {
    shared: Arc<SharedState>,
}

impl StateChecker {
    pub(crate) fn new(shared: Arc<SharedState>) -> Self {
        Self { shared }
    }

    pub fn check(&mut self) -> Result<DeviceStatus, UdpError> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(UdpError::Closed);
        }
        Ok(DeviceStatus::new(self.shared.snapshot()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: u8 = FLAG_ASSIGNED | FLAG_PTP_LOCKED | FLAG_SYNC_READY;
    const LOST_AFTER: Duration = Duration::from_millis(100);

    #[test]
    fn a_ready_reply_is_ready() {
        let mut t = Tracker::new();
        assert!(t.replied(2, 2, READY));
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_pulse_is_syncing() {
        let mut t = Tracker::new();
        assert!(t.replied(0, 0, FLAG_ASSIGNED));
        assert_eq!(t.state(), DeviceState::Syncing);
        assert!(t.replied(0, 0, READY));
        assert_eq!(t.state(), DeviceState::Ready);
    }

    #[test]
    fn a_reply_without_the_ptp_lock_is_syncing() {
        let mut t = Tracker::new();
        assert!(t.replied(1, 1, FLAG_ASSIGNED | FLAG_SYNC_READY));
        assert_eq!(t.state(), DeviceState::Syncing);
        assert!(t.replied(1, 1, FLAG_ASSIGNED | FLAG_PTP_LOCKED));
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
        assert!(!t.replied(1, 1, FLAG_SYNC_READY));
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
        let mut checker = StateChecker::new(Arc::clone(&shared));
        let status = checker.check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Lost]
        );
        shared.close();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
    }
}

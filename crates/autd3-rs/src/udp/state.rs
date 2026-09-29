use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use autd3_cpu_wire::udp::{FLAG_ASSIGNED, FLAG_SYNC_READY};
use autd3_rs_core::{DeviceState, DeviceStatus};

use super::error::UdpError;

pub(crate) const LOST_AFTER_MISSES: u32 = 10;

const STATE_OP: u8 = 0;
const STATE_SAFE_OP: u8 = 1;
const STATE_LOST: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tracker {
    misses: u32,
    lost: bool,
    state: DeviceState,
}

impl Tracker {
    pub(crate) const fn new() -> Self {
        Self {
            misses: 0,
            lost: false,
            state: DeviceState::Op,
        }
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
        self.misses = 0;
        self.state = if flags & FLAG_SYNC_READY != 0 {
            DeviceState::Op
        } else {
            DeviceState::SafeOp
        };
        true
    }

    pub(crate) fn missed(&mut self) {
        self.misses = self.misses.saturating_add(1);
        if self.misses >= LOST_AFTER_MISSES {
            self.lost = true;
            self.state = DeviceState::Lost;
        }
    }

    pub(crate) fn state(self) -> DeviceState {
        self.state
    }
}

#[derive(Debug)]
pub(crate) struct SharedState {
    states: Box<[AtomicU8]>,
    recoveries: AtomicU64,
    closed: AtomicBool,
}

impl SharedState {
    pub(crate) fn new(num_devices: usize) -> Arc<Self> {
        Arc::new(Self {
            states: (0..num_devices).map(|_| AtomicU8::new(STATE_OP)).collect(),
            recoveries: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        })
    }

    pub(crate) fn publish(&self, device: usize, state: DeviceState) {
        let encoded = match state {
            DeviceState::Op => STATE_OP,
            DeviceState::Lost => STATE_LOST,
            _ => STATE_SAFE_OP,
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
                STATE_OP => DeviceState::Op,
                STATE_LOST => DeviceState::Lost,
                _ => DeviceState::SafeOp,
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
        Ok(DeviceStatus::new(
            self.shared.snapshot(),
            self.shared.recoveries.load(Ordering::Relaxed),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: u8 = FLAG_ASSIGNED | FLAG_SYNC_READY;

    #[test]
    fn a_ready_reply_is_op() {
        let mut t = Tracker::new();
        assert!(t.replied(2, 2, READY));
        assert_eq!(t.state(), DeviceState::Op);
    }

    #[test]
    fn a_reply_without_the_pulse_is_safe_op() {
        let mut t = Tracker::new();
        assert!(t.replied(0, 0, FLAG_ASSIGNED));
        assert_eq!(t.state(), DeviceState::SafeOp);
        assert!(t.replied(0, 0, READY));
        assert_eq!(t.state(), DeviceState::Op);
    }

    #[test]
    fn ten_missed_cycles_make_a_device_lost_for_good() {
        let mut t = Tracker::new();
        for _ in 0..LOST_AFTER_MISSES - 1 {
            t.missed();
            assert_eq!(t.state(), DeviceState::Op);
        }
        assert!(t.replied(1, 1, READY));
        for _ in 0..LOST_AFTER_MISSES {
            t.missed();
        }
        assert_eq!(t.state(), DeviceState::Lost);
        assert!(!t.replied(1, 1, READY));
        assert_eq!(t.state(), DeviceState::Lost);
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
        shared.publish(1, DeviceState::SafeOp);
        shared.publish(2, DeviceState::Lost);
        let mut checker = StateChecker::new(Arc::clone(&shared));
        let status = checker.check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Op, DeviceState::SafeOp, DeviceState::Lost]
        );
        shared.close();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
    }
}

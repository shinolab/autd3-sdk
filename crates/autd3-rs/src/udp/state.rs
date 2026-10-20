use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::{DeviceState, DeviceStatus};

use super::error::UdpError;

const STATE_READY: u8 = 0;
const STATE_SYNCING: u8 = 1;
const STATE_LOST: u8 = 2;
pub(crate) const SYNCED_FLAGS: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);

#[derive(Debug)]
pub(crate) struct PublishedDeviceStates {
    states: Box<[AtomicU8]>,
    closed: AtomicBool,
}

impl PublishedDeviceStates {
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
            DeviceState::Syncing => STATE_SYNCING,
            _ => unreachable!(),
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
    published_states: Arc<PublishedDeviceStates>,
}

impl StateChecker {
    pub(crate) fn new(published_states: Arc<PublishedDeviceStates>) -> Self {
        Self { published_states }
    }

    pub fn check(&self) -> Result<DeviceStatus, UdpError> {
        if self.published_states.closed.load(Ordering::Acquire) {
            return Err(UdpError::Closed);
        }
        Ok(DeviceStatus::new(self.published_states.snapshot()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checker_reads_the_published_states() {
        let published_states = PublishedDeviceStates::new(3);
        published_states.publish(1, DeviceState::Syncing);
        published_states.publish(2, DeviceState::Lost);
        let checker = StateChecker::new(Arc::clone(&published_states));
        let status = checker.check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Lost]
        );
        published_states.close();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
    }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use autd3_cpu_wire::udp::Flags;
use autd3_rs_core::{DeviceMask, DeviceState, DeviceStatus};

use super::error::UdpError;

const STATE_READY: u8 = 0;
const STATE_SYNCING: u8 = 1;
const STATE_LOST: u8 = 2;
const STATE_BITS: u8 = 0b11;
pub(crate) const SYNCED_FLAGS: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);
pub(crate) const PTP_FLAGS: Flags = Flags::PTP_LOCKED.union(Flags::PTP_FAILSAFE);

const _: () = assert!(PTP_FLAGS.bits() & STATE_BITS == 0);

#[derive(Debug)]
pub(crate) struct PublishedDeviceStates {
    states: Box<[AtomicU8]>,
    closed: AtomicBool,
}

impl PublishedDeviceStates {
    pub(crate) fn new(num_devices: usize) -> Arc<Self> {
        Arc::new(Self {
            states: (0..num_devices)
                .map(|_| AtomicU8::new(STATE_READY | Flags::PTP_LOCKED.bits()))
                .collect(),
            closed: AtomicBool::new(false),
        })
    }

    pub(crate) fn publish(&self, device: usize, state: DeviceState, ptp_flags: Flags) {
        let encoded = match state {
            DeviceState::Ready => STATE_READY,
            DeviceState::Lost => STATE_LOST,
            DeviceState::Syncing => STATE_SYNCING,
            _ => unreachable!(),
        };
        self.states[device].store(
            encoded | ptp_flags.intersection(PTP_FLAGS).bits(),
            Ordering::Relaxed,
        );
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    fn snapshot(&self) -> DeviceStatus {
        let mut devices = Vec::with_capacity(self.states.len());
        let mut ptp_locked = DeviceMask::default();
        let mut ptp_failsafe = DeviceMask::default();
        for (device, s) in self.states.iter().enumerate() {
            let encoded = s.load(Ordering::Relaxed);
            devices.push(match encoded & STATE_BITS {
                STATE_READY => DeviceState::Ready,
                STATE_LOST => DeviceState::Lost,
                _ => DeviceState::Syncing,
            });
            let flags = Flags::from_bits_retain(encoded & !STATE_BITS);
            if flags.contains(Flags::PTP_LOCKED) {
                ptp_locked |= DeviceMask::from_iter([device]);
            }
            if flags.contains(Flags::PTP_FAILSAFE) {
                ptp_failsafe |= DeviceMask::from_iter([device]);
            }
        }
        DeviceStatus::new(devices, ptp_locked, ptp_failsafe)
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
        Ok(self.published_states.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checker_reads_the_published_states() {
        let published_states = PublishedDeviceStates::new(3);
        published_states.publish(1, DeviceState::Syncing, Flags::empty());
        published_states.publish(2, DeviceState::Lost, Flags::empty());
        let checker = StateChecker::new(Arc::clone(&published_states));
        let status = checker.check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Lost]
        );
        assert_eq!(status.ptp_locked(), DeviceMask::from_iter([0]));
        assert!(!status.any_ptp_failsafe());
        published_states.close();
        assert!(matches!(checker.check(), Err(UdpError::Closed)));
    }

    #[test]
    fn the_checker_reads_the_published_ptp_flags() {
        let published_states = PublishedDeviceStates::new(3);
        published_states.publish(0, DeviceState::Ready, Flags::PTP_LOCKED);
        published_states.publish(
            1,
            DeviceState::Syncing,
            Flags::PTP_FAILSAFE | Flags::SYNC_READY | Flags::ASSIGNED,
        );
        published_states.publish(2, DeviceState::Ready, PTP_FLAGS);
        let status = StateChecker::new(published_states).check().unwrap();
        assert_eq!(
            status.devices(),
            [DeviceState::Ready, DeviceState::Syncing, DeviceState::Ready]
        );
        assert_eq!(status.ptp_locked(), DeviceMask::from_iter([0, 2]));
        assert_eq!(status.ptp_failsafe(), DeviceMask::from_iter([1, 2]));
    }
}

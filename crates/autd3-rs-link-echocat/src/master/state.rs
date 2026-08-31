use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicU64, Ordering};

use autd3_rs_core::DeviceState;

use crate::reg::AlState;

const UNOBSERVED: u8 = 0xff;

#[derive(Clone)]
pub struct BusState {
    inner: Arc<BusStateInner>,
}

struct BusStateInner {
    al_status: Vec<AtomicU8>,
    al_status_code: Vec<AtomicU16>,
    recoveries: AtomicU64,
}

impl BusState {
    #[must_use]
    pub fn new(devices: usize) -> Self {
        Self {
            inner: Arc::new(BusStateInner {
                al_status: (0..devices).map(|_| AtomicU8::new(UNOBSERVED)).collect(),
                al_status_code: (0..devices).map(|_| AtomicU16::new(0)).collect(),
                recoveries: AtomicU64::new(0),
            }),
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.inner.al_status.len()
    }

    pub fn observe(&self, device: usize, al_status: u8, al_status_code: u16) {
        if let Some(slot) = self.inner.al_status.get(device) {
            slot.store(al_status, Ordering::Relaxed);
        }
        if let Some(slot) = self.inner.al_status_code.get(device) {
            slot.store(al_status_code, Ordering::Relaxed);
        }
    }

    pub fn lose(&self, device: usize) {
        self.observe(device, UNOBSERVED, 0);
    }

    pub fn lose_all(&self) {
        for slot in &self.inner.al_status {
            slot.store(UNOBSERVED, Ordering::Relaxed);
        }
        for slot in &self.inner.al_status_code {
            slot.store(0, Ordering::Relaxed);
        }
    }

    pub fn record_recovery(&self) {
        self.inner.recoveries.fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn recoveries(&self) -> u64 {
        self.inner.recoveries.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn al_status(&self, device: usize) -> Option<u8> {
        self.inner
            .al_status
            .get(device)
            .map(|slot| slot.load(Ordering::Relaxed))
            .filter(|status| *status != UNOBSERVED)
    }

    #[must_use]
    pub fn al_status_code(&self, device: usize) -> Option<u16> {
        self.al_status(device).and_then(|_| {
            self.inner
                .al_status_code
                .get(device)
                .map(|slot| slot.load(Ordering::Relaxed))
        })
    }

    #[must_use]
    pub fn device_state(&self, device: usize) -> DeviceState {
        self.inner
            .al_status
            .get(device)
            .map_or(DeviceState::Lost, |slot| {
                device_state(slot.load(Ordering::Relaxed))
            })
    }

    #[must_use]
    pub fn all_op(&self) -> bool {
        self.inner
            .al_status
            .iter()
            .all(|slot| device_state(slot.load(Ordering::Relaxed)) == DeviceState::Op)
    }

    #[must_use]
    pub fn states(&self) -> Vec<DeviceState> {
        (0..self.num_devices())
            .map(|d| self.device_state(d))
            .collect()
    }
}

#[must_use]
pub fn device_state(al_status: u8) -> DeviceState {
    if al_status == UNOBSERVED {
        return DeviceState::Lost;
    }
    let errored = al_status & AlState::ERROR_FLAG != 0;
    match AlState::from_code(al_status) {
        Some(AlState::Op) if !errored => DeviceState::Op,
        Some(AlState::SafeOp) if errored => DeviceState::SafeOpError,
        Some(AlState::SafeOp) => DeviceState::SafeOp,
        _ => DeviceState::Other(al_status & AlState::STATE_MASK),
    }
}

#[cfg(test)]
mod tests {
    use super::{BusState, device_state};
    use crate::reg::AlState;
    use autd3_rs_core::DeviceState;

    #[test]
    fn al_status_maps_onto_the_link_device_states() {
        assert_eq!(device_state(AlState::Op.code()), DeviceState::Op);
        assert_eq!(device_state(AlState::SafeOp.code()), DeviceState::SafeOp);
        assert_eq!(
            device_state(AlState::SafeOp.code() | AlState::ERROR_FLAG),
            DeviceState::SafeOpError
        );
        assert_eq!(device_state(AlState::Init.code()), DeviceState::Other(0x01));
        assert_eq!(device_state(0xff), DeviceState::Lost);
    }

    #[test]
    fn observations_are_visible_through_every_clone() {
        let state = BusState::new(2);
        let observer = state.clone();
        assert!(!observer.all_op());
        state.observe(0, AlState::Op.code(), 0);
        state.observe(1, AlState::Op.code(), 0);
        assert!(observer.all_op());
        assert_eq!(observer.states(), vec![DeviceState::Op; 2]);

        state.observe(1, AlState::SafeOp.code() | AlState::ERROR_FLAG, 0x001a);
        assert!(!observer.all_op());
        assert_eq!(observer.al_status_code(1), Some(0x001a));
        state.lose(1);
        assert_eq!(
            observer.al_status_code(1),
            None,
            "a device we lost contact with must not keep publishing the last code it latched",
        );
        state.record_recovery();
        assert_eq!(observer.recoveries(), 1);
    }
}

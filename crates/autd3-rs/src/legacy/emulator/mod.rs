mod device;

pub use device::{LegacyDevice, SegmentState, StmKind, default_pulse_width_table};

use core::convert::Infallible;
use std::sync::{Arc, Mutex, PoisonError};

use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, IntoLink, Link};

use crate::legacy::wire::{RX_FRAME_BYTES, TX_FRAME_BYTES};

#[derive(Clone, Debug)]
pub struct LegacyDeviceHandle(Arc<Mutex<LegacyDevice>>);

impl LegacyDeviceHandle {
    pub fn with<R>(&self, f: impl FnOnce(&LegacyDevice) -> R) -> R {
        f(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut LegacyDevice) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

pub struct LegacyAudit {
    devices: Vec<LegacyDeviceHandle>,
}

impl LegacyAudit {
    #[must_use]
    pub fn new(num_transducers: impl IntoIterator<Item = usize>) -> Self {
        Self {
            devices: num_transducers
                .into_iter()
                .enumerate()
                .map(|(idx, n)| LegacyDeviceHandle(Arc::new(Mutex::new(LegacyDevice::new(idx, n)))))
                .collect(),
        }
    }

    #[must_use]
    pub fn devices(&self) -> Vec<LegacyDeviceHandle> {
        self.devices.clone()
    }

    #[must_use]
    pub fn device(&self, idx: usize) -> LegacyDeviceHandle {
        self.devices[idx].clone()
    }
}

impl Link for LegacyAudit {
    type Error = Infallible;
    type Checker = ConstStateChecker;

    fn num_devices(&self) -> usize {
        self.devices.len()
    }

    fn state_checker(&self) -> Self::Checker {
        ConstStateChecker::new(self.devices.len())
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        for ((device, tx), rx) in self.devices.iter().zip(tx).zip(rx) {
            device.with_mut(|d| d.cycle(tx, rx));
        }
        Ok(CycleOutcome { rx_valid: true })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LegacyNop;

impl LegacyNop {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IntoLink for LegacyNop {
    type Link = LegacyAudit;

    async fn into_link(
        self,
        geometry: &Geometry,
    ) -> Result<LegacyAudit, autd3_rs_core::error::LinkError> {
        Ok(LegacyAudit::new(
            geometry.iter().map(Device::num_transducers),
        ))
    }
}

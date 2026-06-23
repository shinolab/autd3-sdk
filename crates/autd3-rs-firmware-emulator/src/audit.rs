use core::convert::Infallible;

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::device::Device;

pub struct Audit {
    devices: Vec<Device>,
}

impl Audit {
    #[must_use]
    pub fn new(num_transducers: impl IntoIterator<Item = usize>) -> Self {
        Self {
            devices: num_transducers.into_iter().map(Device::new).collect(),
        }
    }

    #[must_use]
    pub fn device(&self, idx: usize) -> &Device {
        &self.devices[idx]
    }

    #[must_use]
    pub fn device_mut(&mut self, idx: usize) -> &mut Device {
        &mut self.devices[idx]
    }
}

impl Link for Audit {
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
        for ((device, tx), rx) in self.devices.iter_mut().zip(tx).zip(rx) {
            device.send(tx).write_to(rx);
        }
        Ok(CycleOutcome { rx_valid: true })
    }
}

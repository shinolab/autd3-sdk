use core::convert::Infallible;

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};

use crate::device::Device;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fault {
    pub drop_frames: usize,
    pub invalid_cycles: usize,
    pub device: Option<usize>,
}

pub struct Audit {
    devices: Vec<Device>,
    fault: Fault,
}

impl Audit {
    #[must_use]
    pub fn new(num_transducers: impl IntoIterator<Item = usize>) -> Self {
        Self {
            devices: num_transducers.into_iter().map(Device::new).collect(),
            fault: Fault::default(),
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

    pub fn inject(&mut self, fault: Fault) {
        self.fault = fault;
    }

    #[must_use]
    pub fn pending_fault(&self) -> Fault {
        self.fault
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
        let invalid = self.fault.invalid_cycles > 0;
        self.fault.invalid_cycles = self.fault.invalid_cycles.saturating_sub(1);
        let dropping = self.fault.drop_frames > 0;
        self.fault.drop_frames = self.fault.drop_frames.saturating_sub(1);

        for (i, ((device, tx), rx)) in self.devices.iter_mut().zip(tx).zip(rx).enumerate() {
            let targeted = self.fault.device.is_none_or(|d| d == i);
            if invalid || (dropping && targeted) {
                device.rx().write_to(rx);
            } else {
                device.send(tx).write_to(rx);
            }
        }
        Ok(CycleOutcome::new(!invalid))
    }
}

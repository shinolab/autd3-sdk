use core::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_simulator_protocol::{DeviceState, TransState};

use crate::control::ControlState;
use crate::emulator::{extract_device_states, extract_states_into};

pub type SharedStates = Arc<Mutex<Vec<TransState>>>;
pub type SharedDeviceStates = Arc<Mutex<Vec<DeviceState>>>;

pub struct EmulatorLink {
    devices: Vec<EmuDevice>,
    states: SharedStates,
    device_states: SharedDeviceStates,
    control: Arc<ControlState>,
    start: Instant,
}

impl EmulatorLink {
    #[must_use]
    pub fn new(
        transducer_counts: impl IntoIterator<Item = usize>,
        states: SharedStates,
        device_states: SharedDeviceStates,
        control: Arc<ControlState>,
    ) -> Self {
        Self {
            devices: transducer_counts.into_iter().map(EmuDevice::new).collect(),
            states,
            device_states,
            control,
            start: Instant::now(),
        }
    }
}

impl Link for EmulatorLink {
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
        let sys_time_ns = u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        for ((device, t), r) in self.devices.iter_mut().zip(tx).zip(rx) {
            device.fpga_mut().update_with_sys_time(sys_time_ns);
            device.send(t).write_to(r);
        }
        if let Ok(mut guard) = self.states.lock() {
            let mod_enabled = self.control.mod_enabled.load(Ordering::Relaxed);
            extract_states_into(&self.devices, &mut guard, mod_enabled);
        }
        if let Ok(mut guard) = self.device_states.lock() {
            *guard = extract_device_states(&self.devices);
        }
        Ok(CycleOutcome { rx_valid: true })
    }
}

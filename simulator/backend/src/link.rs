use core::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use autd3_rs_core::link::{ConstStateChecker, CycleOutcome, Link};
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_core::value::DcSysTime;
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
        }
    }
}

#[cfg(test)]
impl EmulatorLink {
    pub fn devices(&self) -> &[EmuDevice] {
        &self.devices
    }

    pub fn devices_mut(&mut self) -> &mut [EmuDevice] {
        &mut self.devices
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
        let sys_time_ns = DcSysTime::now().map_or(0, DcSysTime::sys_time);
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
        Ok(CycleOutcome::valid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::num::NonZeroU16;

    use autd3_rs::commands::{FixedUpdateRate, Nop, SetOutputMask, SetSilencer};
    use autd3_rs_core::link::{DeviceState, StateCheck};

    use crate::harness::Harness;

    #[test]
    fn control_state_starts_with_modulation_enabled() {
        let control = ControlState::default();
        assert!(control.mod_enabled.load(Ordering::Relaxed));
    }

    #[test]
    fn link_reports_the_devices_it_was_built_with() {
        let h = Harness::new(2);
        assert_eq!(h.link.num_devices(), 2);
        let status = h.link.state_checker().check().unwrap();
        assert_eq!(status.devices(), [DeviceState::Op, DeviceState::Op]);
    }

    #[test]
    fn cycle_publishes_one_state_per_transducer_of_every_device() {
        let mut h = Harness::new(2);
        assert!(h.states().is_empty());

        h.send(Nop);
        let expected: usize = h
            .link
            .devices()
            .iter()
            .map(|d| d.fpga().num_transducers())
            .sum();
        assert_eq!(h.states().len(), expected);
        assert_eq!(h.device_states().len(), 2);
    }

    #[test]
    fn repeated_cycles_do_not_grow_the_state_buffer() {
        let mut h = Harness::new(2);
        h.send(Nop);
        let len = h.states().len();
        h.send(Nop);
        h.send(Nop);
        assert_eq!(h.states().len(), len);
    }

    #[test]
    fn output_mask_is_reflected_in_the_published_state() {
        let mut h = Harness::new(1);
        h.send(Nop);
        assert!(h.states().iter().all(|s| s.enable));

        let num_transducers = h.fpga().num_transducers();
        let masks = vec![(0..num_transducers).map(|i| i % 2 == 0).collect::<Vec<_>>()];
        h.send(SetOutputMask { masks: &masks });

        let states = h.states();
        assert_eq!(states.len(), num_transducers);
        assert!(states.iter().step_by(2).all(|s| s.enable));
        assert!(states.iter().skip(1).step_by(2).all(|s| !s.enable));
    }

    #[test]
    fn silencer_mode_switches_the_reported_registers() {
        let mut h = Harness::new(1);
        h.send(Nop);
        assert!(!h.device_states()[0].silencer_fixed_update_rate);

        h.send(SetSilencer::new(FixedUpdateRate {
            intensity: NonZeroU16::new(3).unwrap(),
            phase: NonZeroU16::new(5).unwrap(),
        }));

        let state = h.device_states().into_iter().next().unwrap();
        assert!(state.silencer_fixed_update_rate);
        assert_eq!(state.silencer_intensity, 3);
        assert_eq!(state.silencer_phase, 5);
    }
}

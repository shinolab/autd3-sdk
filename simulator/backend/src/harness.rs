use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use autd3_rs::commands::{Command, Distribution};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::{RX_FRAME_BYTES, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs::{DatagramBuilder, Frames};
use autd3_rs_core::link::Link;
use autd3_rs_firmware_emulator::FpgaEmulator;
use autd3_rs_simulator_protocol::{DeviceState, TransState};

use crate::control::ControlState;
use crate::link::{EmulatorLink, SharedDeviceStates, SharedStates};

pub struct Harness {
    pub geometry: Arc<Geometry>,
    pub link: EmulatorLink,
    pub states: SharedStates,
    pub device_states: SharedDeviceStates,
    pub control: Arc<ControlState>,
    seq: u8,
}

impl Harness {
    pub fn new(num_devices: usize) -> Self {
        let geometry = Arc::new(Geometry::new(
            (0..num_devices).map(|_| Autd3::default()).collect(),
        ));
        let states: SharedStates = Arc::new(Mutex::new(Vec::new()));
        let device_states: SharedDeviceStates = Arc::new(Mutex::new(Vec::new()));
        let control = Arc::new(ControlState::default());
        let link = EmulatorLink::new(
            geometry.iter().map(autd3_rs_core::Device::num_transducers),
            Arc::clone(&states),
            Arc::clone(&device_states),
            Arc::clone(&control),
        );
        Self {
            geometry,
            link,
            states,
            device_states,
            control,
            seq: 0,
        }
    }

    pub fn send<'a, C: Command<'a>>(&mut self, cmd: C) {
        let mut builder = DatagramBuilder::new(Arc::clone(&self.geometry));
        builder.push(cmd);
        self.drive(&builder.build().unwrap());
    }

    fn drive(&mut self, frames: &Frames) {
        let num_devices = self.link.num_devices();
        let mut rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
        for frame in frames {
            let datagrams = frame.datagrams();
            let tx: Vec<[u8; TX_FRAME_BYTES]> = (0..num_devices)
                .map(|device| {
                    let datagram = match frame.distribution() {
                        Distribution::Broadcast => &datagrams[0],
                        Distribution::PerDevice => &datagrams[device],
                    };
                    let mut bytes = [0u8; TX_FRAME_BYTES];
                    TxFrame {
                        seq: Seq::new(self.seq),
                        cmd: datagram.cmd,
                        payload: datagram.payload,
                    }
                    .write_to(&mut bytes);
                    bytes
                })
                .collect();
            self.link.cycle(&tx, &mut rx).unwrap();
            self.seq = self.seq.wrapping_add(1);
        }
    }

    pub fn fpga(&self) -> &FpgaEmulator {
        self.link.devices()[0].fpga()
    }

    pub fn fpga_mut(&mut self) -> &mut FpgaEmulator {
        self.link.devices_mut()[0].fpga_mut()
    }

    pub fn set_mod_enabled(&self, enabled: bool) {
        self.control.mod_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn states(&self) -> Vec<TransState> {
        self.states.lock().unwrap().clone()
    }

    pub fn device_states(&self) -> Vec<DeviceState> {
        self.device_states.lock().unwrap().clone()
    }
}

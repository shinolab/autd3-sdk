use std::sync::Arc;
use std::sync::atomic::Ordering;

use autd3_rs::commands::{Command, Distribution};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::{FRAME_BYTES_MAX, Seq, TxFrame};
use autd3_rs::value::SysTime;
use autd3_rs::{DatagramBuilder, Frames};
use autd3_rs_firmware_emulator::{Device as EmuDevice, FpgaEmulator};
use autd3_rs_simulator_protocol::{DeviceState, TransState};

use crate::control::ControlState;
use crate::emulator::{extract_device_states, extract_states_into};

pub struct Harness {
    pub geometry: Arc<Geometry>,
    pub devices: Vec<EmuDevice>,
    pub control: Arc<ControlState>,
    seq: u8,
}

impl Harness {
    pub fn new(num_devices: usize) -> Self {
        let geometry = Arc::new(Geometry::new(
            (0..num_devices).map(|_| Autd3::default()).collect(),
        ));
        let devices = geometry
            .iter()
            .map(|device| EmuDevice::new(device.num_transducers()))
            .collect();
        Self {
            geometry,
            devices,
            control: Arc::new(ControlState::default()),
            seq: 0,
        }
    }

    pub fn send<'a, C: Command<'a>>(&mut self, cmd: C) {
        let mut builder = DatagramBuilder::new(Arc::clone(&self.geometry));
        builder.push(cmd);
        self.drive(&builder.build().unwrap());
    }

    fn drive(&mut self, frames: &Frames) {
        for frame in frames {
            let datagrams = frame.datagrams();
            let sys_time_ns = SysTime::now().map_or(0, SysTime::sys_time);
            for (index, device) in self.devices.iter_mut().enumerate() {
                let datagram = match frame.distribution() {
                    Distribution::Broadcast => &datagrams[0],
                    Distribution::PerDevice => &datagrams[index],
                };
                let mut bytes = [0u8; FRAME_BYTES_MAX];
                TxFrame {
                    seq: Seq::new(self.seq),
                    cmd: datagram.cmd,
                    payload: datagram.payload,
                }
                .write_to(&mut bytes);
                device.fpga_mut().update_with_sys_time(sys_time_ns);
                let _ = device.send(&bytes);
            }
            self.seq = self.seq.wrapping_add(1);
        }
    }

    fn device_refs(&self) -> Vec<&EmuDevice> {
        self.devices.iter().collect()
    }

    pub fn fpga(&self) -> &FpgaEmulator {
        self.devices[0].fpga()
    }

    pub fn fpga_mut(&mut self) -> &mut FpgaEmulator {
        self.devices[0].fpga_mut()
    }

    pub fn set_mod_enabled(&self, enabled: bool) {
        self.control.mod_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn states(&self) -> Vec<TransState> {
        let mut out = Vec::new();
        let mod_enabled = self.control.mod_enabled.load(Ordering::Relaxed);
        extract_states_into(&self.device_refs(), &mut out, mod_enabled);
        out
    }

    pub fn device_states(&self) -> Vec<DeviceState> {
        extract_device_states(&self.device_refs())
    }
}

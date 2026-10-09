use std::sync::Arc;
use std::time::Instant;

use autd3_rs::Frames;
use autd3_rs::commands::Command;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_firmware_emulator::{Device as EmuDevice, FpgaEmulator};
use autd3_rs_simulator_protocol::{DeviceState, TransState};

use crate::emulator::{extract_device_states, extract_states};

pub struct Harness {
    pub geometry: Arc<Geometry>,
    pub devices: Vec<EmuDevice>,
    pub mod_enabled: bool,
    seq: u8,
    boot: Instant,
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
            mod_enabled: true,
            seq: 0,
            boot: Instant::now(),
        }
    }

    pub fn send<'a, C: Command<'a>>(&mut self, cmd: C) {
        self.drive(&Frames::encode(&self.geometry, cmd).unwrap());
    }

    fn drive(&mut self, frames: &Frames) {
        for frame in frames {
            let sys_time_ns = u64::try_from(self.boot.elapsed().as_nanos()).unwrap_or(u64::MAX);
            for (index, device) in self.devices.iter_mut().enumerate() {
                let datagram = frame.datagram_for(index);
                let bytes = [&[self.seq, datagram.cmd.as_u8()], datagram.payload()].concat();
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

    pub fn states(&self) -> Vec<TransState> {
        extract_states(&self.device_refs(), self.mod_enabled)
    }

    pub fn device_states(&self) -> Vec<DeviceState> {
        extract_device_states(&self.device_refs())
    }
}

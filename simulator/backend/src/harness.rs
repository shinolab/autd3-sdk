use std::sync::Arc;
use std::time::Instant;

use autd3_rs::Frames;
use autd3_rs::commands::Command;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_firmware_emulator::{Device as EmuDevice, FpgaEmulator};
use autd3_rs_simulator_protocol::{DeviceState, TransState};

use crate::emulator::{extract_device_states, extract_states};

pub(crate) struct Harness {
    pub geometry: Arc<Geometry>,
    pub devices: Vec<EmuDevice>,
    pub mod_enabled: bool,
    boot: Instant,
}

impl Harness {
    pub(crate) fn new(num_devices: usize) -> Self {
        let geometry = Arc::new(Geometry::new((0..num_devices).map(|_| Autd3::default())));
        let devices = geometry
            .iter()
            .map(|device| EmuDevice::new(device.num_transducers()))
            .collect();
        Self {
            geometry,
            devices,
            mod_enabled: true,
            boot: Instant::now(),
        }
    }

    pub(crate) fn send<C: Command>(&mut self, cmd: C) {
        self.drive(&Frames::encode(&self.geometry, cmd).unwrap());
    }

    fn drive(&mut self, frames: &Frames) {
        for frame in frames {
            let sys_time_ns = u64::try_from(self.boot.elapsed().as_nanos()).unwrap_or(u64::MAX);
            for (index, device) in self.devices.iter_mut().enumerate() {
                let datagram = frame.datagram_for(index);
                let bytes = [&[0, datagram.cmd.as_u8(), 0, 0, 0, 0], datagram.payload()].concat();
                device.fpga_mut().update_with_sys_time(sys_time_ns);
                let _ = device.send(&bytes);
            }
        }
    }

    fn device_refs(&self) -> Vec<&EmuDevice> {
        self.devices.iter().collect()
    }

    pub(crate) fn fpga(&self) -> &FpgaEmulator {
        self.devices[0].fpga()
    }

    pub(crate) fn states(&self) -> Vec<TransState> {
        extract_states(&self.device_refs(), self.mod_enabled)
    }

    pub(crate) fn device_states(&self) -> Vec<DeviceState> {
        extract_device_states(&self.device_refs())
    }
}

use std::io;

use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub(crate) struct Scene {
    port: u16,
    emulator: Option<UdpEmulator>,
}

impl Scene {
    pub(crate) fn new(port: u16, num_devices: usize) -> io::Result<Self> {
        let emulator = UdpEmulator::spawn_simulator(port, num_devices)?;
        Ok(Self {
            port: emulator.addr().port(),
            emulator: Some(emulator),
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn resize(&mut self, num_devices: usize) -> io::Result<()> {
        let previous = match self.emulator.take() {
            Some(emulator) if emulator.num_devices() == num_devices => {
                self.emulator = Some(emulator);
                return Ok(());
            }
            Some(emulator) => Some(emulator.num_devices()),
            None => None,
        };
        match UdpEmulator::spawn_simulator(self.port, num_devices) {
            Ok(rebuilt) => {
                self.emulator = Some(rebuilt);
                Ok(())
            }
            Err(e) => {
                self.emulator = previous.and_then(|num_devices| {
                    UdpEmulator::spawn_simulator(self.port, num_devices).ok()
                });
                Err(e)
            }
        }
    }

    pub(crate) fn sample<R>(&self, f: impl FnOnce(&[&EmuDevice]) -> R) -> Option<R> {
        self.emulator
            .as_ref()
            .map(|emulator| emulator.with_devices(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resize_to_the_same_number_of_devices_keeps_the_emulator() {
        let mut scene = Scene::new(0, 2).unwrap();
        let addr = scene.emulator.as_ref().unwrap().device_addr(0);
        scene.resize(2).unwrap();
        assert_eq!(scene.emulator.as_ref().unwrap().device_addr(0), addr);
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
    }

    #[test]
    fn a_resize_to_another_number_of_devices_rebuilds_the_emulator() {
        let mut scene = Scene::new(0, 1).unwrap();
        scene.resize(3).unwrap();
        assert_eq!(scene.sample(|devices| devices.len()), Some(3));
        scene.resize(1).unwrap();
        assert_eq!(scene.sample(|devices| devices.len()), Some(1));
    }
}

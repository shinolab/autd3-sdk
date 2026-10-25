use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use autd3_rs::geometry::Geometry;
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use tokio::sync::watch;

use crate::emulator::geometry_msg;

pub(crate) struct Scene {
    port: u16,
    emulator: Mutex<Option<UdpEmulator>>,
    geometry: watch::Sender<Arc<str>>,
}

impl Scene {
    pub(crate) fn new(port: u16, geometry: &Geometry) -> io::Result<Arc<Self>> {
        let emulator = UdpEmulator::spawn_simulator(port, geometry.num_devices())?;
        let (geometry, _) = watch::channel(geometry_json(geometry)?);
        Ok(Arc::new(Self {
            port: emulator.addr().port(),
            emulator: Mutex::new(Some(emulator)),
            geometry,
        }))
    }

    fn emulator(&self) -> MutexGuard<'_, Option<UdpEmulator>> {
        self.emulator.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn geometry(&self) -> watch::Receiver<Arc<str>> {
        self.geometry.subscribe()
    }

    fn republish(&self) {
        self.geometry.send_modify(|_| {});
    }

    pub(crate) fn replace(&self, geometry: &Geometry) -> io::Result<()> {
        let json = geometry_json(geometry)?;
        let mut emulator = self.emulator();
        if emulator.as_ref().map(UdpEmulator::num_devices) != Some(geometry.num_devices())
            && let Err(e) = self.rebuild(&mut emulator, geometry.num_devices())
        {
            self.republish();
            return Err(e);
        }
        self.geometry.send_replace(json);
        Ok(())
    }

    fn rebuild(&self, emulator: &mut Option<UdpEmulator>, num_devices: usize) -> io::Result<()> {
        let previous = emulator.take().map(|old| old.num_devices());
        match UdpEmulator::spawn_simulator(self.port, num_devices) {
            Ok(rebuilt) => {
                *emulator = Some(rebuilt);
                Ok(())
            }
            Err(e) => {
                *emulator = previous.and_then(|num_devices| {
                    UdpEmulator::spawn_simulator(self.port, num_devices).ok()
                });
                Err(e)
            }
        }
    }

    pub(crate) fn sample<R>(&self, f: impl FnOnce(&[&EmuDevice]) -> R) -> Option<R> {
        self.emulator()
            .as_ref()
            .map(|emulator| emulator.with_devices(f))
    }
}

fn geometry_json(geometry: &Geometry) -> io::Result<Arc<str>> {
    Ok(serde_json::to_string(&geometry_msg(geometry))?.into())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use autd3_rs::geometry::{Autd3, Point3, UnitQuaternion};
    use autd3_rs_simulator_protocol::{DevicePose, ServerMsg};

    use crate::emulator::device_poses;

    pub(crate) fn scene(num_devices: usize) -> Arc<Scene> {
        Scene::new(0, &Geometry::new(vec![Autd3::default(); num_devices])).unwrap()
    }

    fn shifted(num_devices: usize, dy: f32) -> Geometry {
        Geometry::new((0..num_devices).map(|i| {
            Autd3::new(
                Point3::new(i as f32 * Autd3::DEVICE_WIDTH, dy, 0.0),
                UnitQuaternion::identity(),
            )
        }))
    }

    pub(crate) fn published_poses(rx: &mut watch::Receiver<Arc<str>>) -> Vec<DevicePose> {
        let json = rx.borrow_and_update().clone();
        match serde_json::from_str(&json).unwrap() {
            ServerMsg::Geometry { devices, .. } => devices,
            _ => panic!("expected a geometry message"),
        }
    }

    #[test]
    fn an_update_with_the_same_number_of_devices_keeps_the_emulator() {
        let scene = scene(2);
        let addr = scene.emulator().as_ref().unwrap().device_addr(0);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        let moved = shifted(2, 50.0);
        scene.replace(&moved).unwrap();
        assert_eq!(scene.emulator().as_ref().unwrap().device_addr(0), addr);
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx), device_poses(&moved));
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
    }

    #[test]
    fn an_update_with_another_number_of_devices_rebuilds_the_emulator() {
        let scene = scene(1);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        scene.replace(&shifted(3, 0.0)).unwrap();
        assert_eq!(scene.sample(|devices| devices.len()), Some(3));
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx).len(), 3);
    }

    #[test]
    fn the_later_update_wins() {
        let scene = scene(1);
        let mut rx = scene.geometry();
        scene.replace(&shifted(2, 5.0)).unwrap();
        let later = shifted(1, 9.0);
        scene.replace(&later).unwrap();
        assert_eq!(published_poses(&mut rx), device_poses(&later));
    }
}

use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use autd3_rs::geometry::Geometry;
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use tokio::sync::watch;

use crate::emulator::geometry_msg;

pub(crate) type Spawn = Box<dyn Fn(usize) -> io::Result<UdpEmulator> + Send + Sync>;

pub(crate) struct Scene {
    spawn: Spawn,
    emulator: Mutex<Option<UdpEmulator>>,
    geometry: watch::Sender<Arc<str>>,
}

impl Scene {
    pub(crate) fn new(spawn: Spawn, geometry: &Geometry) -> io::Result<Arc<Self>> {
        let emulator = spawn(geometry.num_devices())?;
        let (geometry, _) = watch::channel(geometry_json(geometry)?);
        Ok(Arc::new(Self {
            spawn,
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
        match (self.spawn)(num_devices) {
            Ok(rebuilt) => {
                *emulator = Some(rebuilt);
                Ok(())
            }
            Err(e) => {
                *emulator = previous.and_then(|num_devices| (self.spawn)(num_devices).ok());
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

    use std::sync::atomic::{AtomicUsize, Ordering};

    use autd3_rs::geometry::{Autd3, Point3, UnitQuaternion};
    use autd3_rs_simulator_protocol::{DevicePose, ServerMsg};

    use crate::emulator::device_poses;

    pub(crate) fn counting_scene(num_devices: usize) -> (Arc<Scene>, Arc<AtomicUsize>) {
        let spawns = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&spawns);
        let scene = Scene::new(
            Box::new(move |num_devices| {
                counter.fetch_add(1, Ordering::Relaxed);
                UdpEmulator::spawn(num_devices)
            }),
            &Geometry::new(vec![Autd3::default(); num_devices]),
        )
        .unwrap();
        (scene, spawns)
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
        let (scene, spawns) = counting_scene(2);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        let moved = shifted(2, 50.0);
        scene.replace(&moved).unwrap();
        assert_eq!(spawns.load(Ordering::Relaxed), 1);
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx), device_poses(&moved));
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
    }

    #[test]
    fn an_update_with_another_number_of_devices_rebuilds_the_emulator() {
        let (scene, spawns) = counting_scene(1);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        scene.replace(&shifted(3, 0.0)).unwrap();
        assert_eq!(spawns.load(Ordering::Relaxed), 2);
        assert_eq!(scene.sample(|devices| devices.len()), Some(3));
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx).len(), 3);
    }

    #[test]
    fn a_failed_rebuild_republishes_the_current_geometry() {
        let scene = Scene::new(
            Box::new(|num_devices| {
                if num_devices == 3 {
                    Err(io::Error::from(io::ErrorKind::AddrInUse))
                } else {
                    UdpEmulator::spawn(num_devices)
                }
            }),
            &shifted(2, 0.0),
        )
        .unwrap();
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        assert!(scene.replace(&shifted(3, 0.0)).is_err());
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx), device_poses(&shifted(2, 0.0)));
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
    }

    #[test]
    fn the_later_update_wins() {
        let (scene, _) = counting_scene(1);
        let mut rx = scene.geometry();
        scene.replace(&shifted(2, 5.0)).unwrap();
        let later = shifted(1, 9.0);
        scene.replace(&later).unwrap();
        assert_eq!(published_poses(&mut rx), device_poses(&later));
    }
}

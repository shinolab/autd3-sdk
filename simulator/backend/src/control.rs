use std::io;
use std::net::{SocketAddr, SocketAddrV6, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use autd3_rs::geometry::Geometry;
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_simulator_control::{Status, decode_geometry, encode_reply};
use tokio::sync::watch;

use crate::emulator::geometry_msg;
use crate::geometry_file;

const DATAGRAM_BYTES_MAX: usize = 8192;
const ERROR_BACKOFF_MIN: Duration = Duration::from_millis(1);
const ERROR_BACKOFF_MAX: Duration = Duration::from_secs(1);

pub(crate) type Spawn = Box<dyn Fn(usize) -> io::Result<UdpEmulator> + Send + Sync>;

pub(crate) struct Scene {
    spawn: Spawn,
    saved_geometry: Option<PathBuf>,
    emulator: Mutex<Option<UdpEmulator>>,
    geometry: watch::Sender<Arc<str>>,
}

impl Scene {
    pub(crate) fn new(
        spawn: Spawn,
        geometry: &Geometry,
        saved_geometry: Option<PathBuf>,
    ) -> io::Result<Arc<Self>> {
        let emulator = spawn(geometry.num_devices())?;
        let (geometry, _) = watch::channel(geometry_json(geometry)?);
        Ok(Arc::new(Self {
            spawn,
            saved_geometry,
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

    pub(crate) fn replace_from_control(&self, geometry: &Geometry) -> io::Result<()> {
        let json = geometry_json(geometry)?;
        let mut emulator = self.emulator();
        self.rebuild(&mut emulator, geometry.num_devices())?;
        self.geometry.send_replace(json);
        Ok(())
    }

    fn republish(&self) {
        self.geometry.send_modify(|_| {});
    }

    pub(crate) fn replace_from_gui(&self, geometry: &Geometry) -> io::Result<()> {
        let json = geometry_json(geometry)?;
        let mut emulator = self.emulator();
        if emulator.as_ref().map(UdpEmulator::num_devices) != Some(geometry.num_devices())
            && let Err(e) = self.rebuild(&mut emulator, geometry.num_devices())
        {
            self.republish();
            return Err(e);
        }
        self.geometry.send_replace(json);
        if let Some(path) = &self.saved_geometry
            && let Err(e) = geometry_file::save(path, geometry)
        {
            tracing::warn!("failed to save the geometry to {}: {e}", path.display());
        }
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

fn apply(scene: &Scene, datagram: &[u8]) -> Status {
    let geometry = match decode_geometry(datagram) {
        Ok(geometry) => geometry,
        Err(e) => {
            tracing::warn!("refused a geometry: {e}");
            return Status::Invalid;
        }
    };
    match scene.replace_from_control(&geometry) {
        Ok(()) => {
            tracing::info!(
                "the control port set the geometry: {} emulated device(s)",
                geometry.num_devices()
            );
            Status::Ok
        }
        Err(e) => {
            tracing::error!("failed to rebuild the emulated devices: {e}");
            Status::Failed
        }
    }
}

fn next_backoff(previous: Option<Duration>) -> Duration {
    previous.map_or(ERROR_BACKOFF_MIN, |previous| {
        previous.saturating_mul(2).min(ERROR_BACKOFF_MAX)
    })
}

pub(crate) fn spawn(scene: Arc<Scene>, addr: SocketAddrV6) -> io::Result<SocketAddrV6> {
    let socket = UdpSocket::bind(addr)?;
    let SocketAddr::V6(addr) = socket.local_addr()? else {
        return Err(io::Error::from(io::ErrorKind::AddrNotAvailable));
    };
    std::thread::Builder::new()
        .name("simulator-control".into())
        .spawn(move || {
            let mut buf = vec![0u8; DATAGRAM_BYTES_MAX];
            let mut backoff = None;
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, src)) => {
                        backoff = None;
                        let status = apply(&scene, &buf[..len]);
                        if let Err(e) = socket.send_to(&encode_reply(status), src) {
                            tracing::debug!("failed to answer {src}: {e}");
                        }
                    }
                    Err(e) => {
                        let wait = next_backoff(backoff);
                        tracing::debug!("control socket error: {e}; retrying in {wait:?}");
                        std::thread::sleep(wait);
                        backoff = Some(wait);
                    }
                }
            }
        })?;
    Ok(addr)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use autd3_rs::geometry::{Autd3, Point3, UnitQuaternion};
    use autd3_rs_simulator_protocol::{DevicePose, ServerMsg};

    use crate::emulator::device_poses;
    use crate::geometry_file::tests::ScratchDir;

    pub(crate) fn counting_scene(
        num_devices: usize,
        saved_geometry: Option<PathBuf>,
    ) -> (Arc<Scene>, Arc<AtomicUsize>) {
        let spawns = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&spawns);
        let scene = Scene::new(
            Box::new(move |num_devices| {
                counter.fetch_add(1, Ordering::Relaxed);
                UdpEmulator::spawn(num_devices)
            }),
            &Geometry::new(vec![Autd3::default(); num_devices]),
            saved_geometry,
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
    fn a_gui_update_with_the_same_number_of_devices_keeps_the_emulator() {
        let (scene, spawns) = counting_scene(2, None);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        let moved = shifted(2, 50.0);
        scene.replace_from_gui(&moved).unwrap();
        assert_eq!(spawns.load(Ordering::Relaxed), 1);
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx), device_poses(&moved));
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
    }

    #[test]
    fn a_gui_update_with_another_number_of_devices_rebuilds_the_emulator() {
        let (scene, spawns) = counting_scene(1, None);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        scene.replace_from_gui(&shifted(3, 0.0)).unwrap();
        assert_eq!(spawns.load(Ordering::Relaxed), 2);
        assert_eq!(scene.sample(|devices| devices.len()), Some(3));
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx).len(), 3);
    }

    #[test]
    fn a_control_update_always_rebuilds_the_emulator() {
        let (scene, spawns) = counting_scene(2, None);
        let mut rx = scene.geometry();
        let moved = shifted(2, 10.0);
        scene.replace_from_control(&moved).unwrap();
        assert_eq!(spawns.load(Ordering::Relaxed), 2);
        assert_eq!(published_poses(&mut rx), device_poses(&moved));
    }

    #[test]
    fn only_a_gui_update_is_saved() {
        let scratch = ScratchDir::new("scene-save");
        let path = scratch.file();
        let (scene, _) = counting_scene(1, Some(path.clone()));
        scene.replace_from_control(&shifted(2, 0.0)).unwrap();
        assert!(!path.exists());
        let from_gui = shifted(2, 25.0);
        scene.replace_from_gui(&from_gui).unwrap();
        assert_eq!(
            device_poses(&geometry_file::load(&path).unwrap()),
            device_poses(&from_gui)
        );
        scene.replace_from_control(&shifted(3, 0.0)).unwrap();
        assert_eq!(
            device_poses(&geometry_file::load(&path).unwrap()),
            device_poses(&from_gui)
        );
    }

    #[test]
    fn a_failed_gui_rebuild_republishes_the_current_geometry_and_saves_nothing() {
        let scratch = ScratchDir::new("scene-failed-rebuild");
        let path = scratch.file();
        let scene = Scene::new(
            Box::new(|num_devices| {
                if num_devices == 3 {
                    Err(io::Error::from(io::ErrorKind::AddrInUse))
                } else {
                    UdpEmulator::spawn(num_devices)
                }
            }),
            &shifted(2, 0.0),
            Some(path.clone()),
        )
        .unwrap();
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        assert!(scene.replace_from_gui(&shifted(3, 0.0)).is_err());
        assert!(rx.has_changed().unwrap());
        assert_eq!(published_poses(&mut rx), device_poses(&shifted(2, 0.0)));
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
        assert!(!path.exists());
    }

    #[test]
    fn the_later_update_wins_across_both_paths() {
        let (scene, _) = counting_scene(1, None);
        let mut rx = scene.geometry();
        scene.replace_from_gui(&shifted(2, 5.0)).unwrap();
        let from_control = shifted(1, 7.0);
        scene.replace_from_control(&from_control).unwrap();
        assert_eq!(published_poses(&mut rx), device_poses(&from_control));
        let from_gui = shifted(1, 9.0);
        scene.replace_from_gui(&from_gui).unwrap();
        assert_eq!(published_poses(&mut rx), device_poses(&from_gui));
    }

    #[test]
    fn the_wait_after_an_error_doubles_up_to_the_cap() {
        let mut backoff = None;
        let mut waits = Vec::new();
        for _ in 0..12 {
            let wait = next_backoff(backoff);
            waits.push(wait);
            backoff = Some(wait);
        }
        assert_eq!(waits[0], ERROR_BACKOFF_MIN);
        assert!(
            waits
                .windows(2)
                .all(|w| w[1] == (w[0] * 2).min(ERROR_BACKOFF_MAX))
        );
        assert_eq!(waits[11], ERROR_BACKOFF_MAX);
    }
}

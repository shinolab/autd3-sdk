use std::io;
use std::net::{SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use autd3_rs::geometry::Geometry;
use autd3_rs_core::simulator::{Status, decode_geometry, encode_reply};
use autd3_rs_firmware_emulator::Device as EmuDevice;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use tokio::sync::watch;

use crate::emulator::geometry_msg;

const DATAGRAM_BYTES_MAX: usize = 8192;

pub type Spawn = fn(usize) -> io::Result<UdpEmulator>;

pub struct Scene {
    spawn: Spawn,
    emulator: Mutex<Option<UdpEmulator>>,
    geometry: watch::Sender<Arc<str>>,
}

impl Scene {
    pub fn new(spawn: Spawn, geometry: &Geometry) -> io::Result<Arc<Self>> {
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

    pub fn geometry(&self) -> watch::Receiver<Arc<str>> {
        self.geometry.subscribe()
    }

    pub fn replace(&self, geometry: &Geometry) -> io::Result<()> {
        let json = geometry_json(geometry)?;
        let mut emulator = self.emulator();
        let previous = emulator.take().map(|old| old.num_devices());
        match (self.spawn)(geometry.num_devices()) {
            Ok(rebuilt) => {
                *emulator = Some(rebuilt);
                self.geometry.send_replace(json);
                Ok(())
            }
            Err(e) => {
                *emulator = previous.and_then(|num_devices| (self.spawn)(num_devices).ok());
                Err(e)
            }
        }
    }

    pub fn sample<R>(&self, f: impl FnOnce(&[&EmuDevice]) -> R) -> Option<R> {
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
    match scene.replace(&geometry) {
        Ok(()) => {
            tracing::info!(
                "a client set the geometry: {} emulated device(s)",
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

pub fn spawn(scene: Arc<Scene>, addr: SocketAddrV6) -> io::Result<SocketAddrV6> {
    let socket = UdpSocket::bind(addr)?;
    let SocketAddr::V6(addr) = socket.local_addr()? else {
        return Err(io::Error::from(io::ErrorKind::AddrNotAvailable));
    };
    std::thread::Builder::new()
        .name("simulator-control".into())
        .spawn(move || {
            let mut buf = vec![0u8; DATAGRAM_BYTES_MAX];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, src)) => {
                        let status = apply(&scene, &buf[..len]);
                        if let Err(e) = socket.send_to(&encode_reply(status), src) {
                            tracing::debug!("failed to answer {src}: {e}");
                        }
                    }
                    Err(e) => tracing::debug!("control socket error: {e}"),
                }
            }
        })?;
    Ok(addr)
}

mod engine;
mod queue;

use std::net::SocketAddrV6;
use std::sync::Arc;
use std::time::Instant;

use autd3_rs_core::rt::oneshot;
use autd3_rs_core::{BusStats, DeviceClock};

use crate::client::{ClientConfig, MAX_DEVICES};
use crate::error::{Error, NetworkCause, PayloadError};
use crate::transport::Bus;
use crate::udp::{StateChecker, TransportOption, UdpBus};

pub(crate) use engine::Engine;
pub use queue::Notified;
use queue::Queue;
pub(crate) use queue::{CmdMessage, Connect, Link, TransportConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    Next(Instant),
    Closed,
}

pub struct Driver {
    engine: Engine<UdpBus>,
}

pub struct Connector {
    pub(crate) link: Link,
    pub(crate) done: oneshot::Receiver<Option<NetworkCause>>,
    pub(crate) num_devices: usize,
    pub(crate) stats: BusStats,
    pub(crate) device_clock: Option<DeviceClock>,
}

impl Connector {
    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }
}

impl core::fmt::Debug for Connector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Connector")
            .field("num_devices", &self.num_devices)
            .finish_non_exhaustive()
    }
}

pub(crate) fn attach<B: Bus>(bus: B) -> (Engine<B>, Connector) {
    let queue = Queue::new();
    let (done_tx, done_rx) = oneshot::channel();
    let connector = Connector {
        link: Link {
            queue: Arc::clone(&queue),
        },
        done: done_rx,
        num_devices: bus.num_devices(),
        stats: bus.stats(),
        device_clock: bus.device_clock(),
    };
    let engine = Engine::new(bus, queue, done_tx, ClientConfig::default().transport());
    (engine, connector)
}

pub(crate) fn check_device_count(num_devices: usize) -> Result<(), Error> {
    if num_devices == 0 || num_devices > MAX_DEVICES {
        return Err(PayloadError::DeviceCountOutOfRange {
            got: num_devices,
            max: MAX_DEVICES,
        }
        .into());
    }
    Ok(())
}

impl Driver {
    pub fn open(option: &TransportOption, num_devices: usize) -> Result<(Self, Connector), Error> {
        check_device_count(num_devices)?;
        let bus = UdpBus::open(option, num_devices)?;
        let (engine, connector) = attach(bus);
        Ok((Self { engine }, connector))
    }

    pub fn poll(&mut self) -> Poll {
        self.engine.poll()
    }

    pub fn wait(&mut self, deadline: Instant) {
        self.engine.wait(deadline);
    }

    pub fn notified(&self) -> Notified {
        Notified::new(Arc::clone(self.engine.queue()), self.engine.seen())
    }

    pub fn run(&mut self) -> Result<(), Error> {
        self.engine.run()
    }

    pub fn close(mut self) -> Result<(), Error> {
        self.engine.close()
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.engine.is_closed()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        self.engine.bus().state_checker()
    }

    #[must_use]
    pub fn stats(&self) -> BusStats {
        self.engine.bus().stats()
    }

    #[must_use]
    pub fn device_clock(&self) -> DeviceClock {
        self.engine.bus().device_clock()
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.engine.bus().num_devices()
    }

    #[must_use]
    pub fn units(&self) -> &[SocketAddrV6] {
        self.engine.bus().units()
    }
}

impl core::fmt::Debug for Driver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Driver")
            .field("num_devices", &self.num_devices())
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Driver {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.engine.bus().as_fd()
    }
}

#[cfg(unix)]
impl std::os::fd::AsRawFd for Driver {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.engine.bus().as_raw_fd()
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Driver {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        self.engine.bus().as_socket()
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawSocket for Driver {
    fn as_raw_socket(&self) -> std::os::windows::io::RawSocket {
        self.engine.bus().as_raw_socket()
    }
}

use std::sync::Arc;
use std::time::Instant;

use autd3_rs_core::{BusStats, DeviceClock};

use crate::error::Error;
use crate::geometry::Geometry;
use crate::link::conn::{self, ServerConn, ServerReader};
use crate::link::{Link, LinkError};
use crate::udp::frame_buf::FrameBuf;
use crate::udp::{Pump, Sent, StateChecker, UdpBus, UdpError};

pub(crate) enum Transport {
    Udp(Arc<UdpBus>),
    Server(ServerConn),
}

pub(crate) enum Port {
    Udp(Pump),
    Server(ServerReader),
}

pub(crate) enum Dispatch {
    Settled(Result<Sent, UdpError>),
    Deferred,
    Failed(LinkError),
}

impl Transport {
    pub(crate) fn open(link: &Link, geometry: &Geometry) -> Result<(Arc<Self>, Port), Error> {
        match link {
            Link::Udp(option) => {
                let bus = Arc::new(UdpBus::open_with_geometry(option, geometry)?);
                let pump = Pump::new(Arc::clone(&bus));
                Ok((Arc::new(Transport::Udp(bus)), Port::Udp(pump)))
            }
            Link::Server(server) => {
                let (conn, reader) = conn::connect(server, geometry.num_devices())?;
                Ok((Arc::new(Transport::Server(conn)), Port::Server(reader)))
            }
        }
    }

    pub(crate) fn reserve_msg_id(&self) -> u16 {
        match self {
            Transport::Udp(bus) => bus.reserve_msg_id(),
            Transport::Server(conn) => conn.reserve_msg_id(),
        }
    }

    pub(crate) fn send(&self, msg_id: u16, broadcast: bool, frames: &[FrameBuf]) -> Dispatch {
        match self {
            Transport::Udp(bus) => Dispatch::Settled(if broadcast {
                bus.broadcast(msg_id, frames[0].as_ref())
            } else {
                bus.send(msg_id, frames)
            }),
            Transport::Server(conn) => {
                let frames = if broadcast { &frames[..1] } else { frames };
                match conn.send(msg_id, broadcast, frames) {
                    Ok(()) => Dispatch::Deferred,
                    Err(e) => Dispatch::Failed(e.into()),
                }
            }
        }
    }

    pub(crate) fn probe(&self) -> Result<(), Error> {
        match self {
            Transport::Udp(bus) => {
                bus.probe()?;
                if !bus.wake() {
                    tracing::debug!(
                        "could not wake the driver; the recovery settles at its next deadline"
                    );
                }
                Ok(())
            }
            Transport::Server(conn) => conn.recover().map_err(|e| LinkError::from(e).into()),
        }
    }

    pub(crate) fn admit_recovered(&self) {
        match self {
            Transport::Udp(bus) => bus.admit_recovered(),
            Transport::Server(conn) => {
                if let Err(e) = conn.admit_recovered() {
                    tracing::debug!("asking the server to admit the recovered devices failed: {e}");
                }
            }
        }
    }

    pub(crate) fn wake(&self) -> bool {
        match self {
            Transport::Udp(bus) => bus.wake(),
            Transport::Server(conn) => match conn.wake() {
                Ok(()) => true,
                Err(e) => {
                    tracing::debug!("asking the server to wake the receiver failed: {e}");
                    false
                }
            },
        }
    }

    pub(crate) fn state_checker(&self) -> StateChecker {
        match self {
            Transport::Udp(bus) => bus.state_checker(),
            Transport::Server(conn) => conn.state_checker(),
        }
    }

    pub(crate) fn device_clock(&self) -> DeviceClock {
        match self {
            Transport::Udp(bus) => bus.device_clock(),
            Transport::Server(conn) => conn.device_clock(),
        }
    }

    pub(crate) fn stats(&self) -> BusStats {
        match self {
            Transport::Udp(bus) => bus.stats(),
            Transport::Server(conn) => conn.stats(),
        }
    }

    pub(crate) fn close(&self) {
        match self {
            Transport::Udp(bus) => {
                let _ = bus.close();
            }
            Transport::Server(conn) => conn.close(),
        }
    }
}

impl Port {
    pub(crate) fn wait_readable(&mut self, deadline: Instant) -> Result<(), Error> {
        match self {
            Port::Udp(pump) => Ok(pump.bus().wait_readable(deadline)?),
            Port::Server(reader) => reader
                .wait_readable(deadline)
                .map_err(|e| LinkError::from(e).into()),
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Port {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        match self {
            Port::Udp(pump) => std::os::fd::AsFd::as_fd(&**pump.bus()),
            Port::Server(reader) => std::os::fd::AsFd::as_fd(reader.stream()),
        }
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Port {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        match self {
            Port::Udp(pump) => std::os::windows::io::AsSocket::as_socket(&**pump.bus()),
            Port::Server(reader) => std::os::windows::io::AsSocket::as_socket(reader.stream()),
        }
    }
}

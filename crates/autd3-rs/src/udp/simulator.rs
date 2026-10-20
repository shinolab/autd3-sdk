use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::{Duration, Instant};

use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::simulator::{self, Status};

use super::error::UdpError;

const READ_TIMEOUT_WITHOUT_DEADLINE: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SimulatorAddrs {
    pub(crate) data: SocketAddrV6,
    pub(crate) control: SocketAddrV6,
}

impl SimulatorAddrs {
    pub(crate) const DEFAULT: Self = Self::at(simulator::DEFAULT_PORT).unwrap();

    pub(crate) const fn at(port: u16) -> Option<Self> {
        let Some(control) = simulator::control_port(port) else {
            return None;
        };
        Some(Self {
            data: SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0),
            control: SocketAddrV6::new(Ipv6Addr::LOCALHOST, control, 0, 0),
        })
    }
}

pub(crate) fn send_geometry(
    control: SocketAddrV6,
    geometry: &Geometry,
    deadline: Option<Instant>,
) -> Result<(), UdpError> {
    let socket = UdpSocket::bind((Ipv6Addr::LOCALHOST, 0))?;
    socket.send_to(&simulator::encode_geometry(geometry), control)?;
    let mut buf = [0u8; 16];
    loop {
        let remaining = match deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => READ_TIMEOUT_WITHOUT_DEADLINE,
        };
        if remaining.is_zero() {
            return Err(UdpError::SimulatorControlNoResponse(control));
        }
        socket.set_read_timeout(Some(remaining))?;
        let (len, src) = match socket.recv_from(&mut buf) {
            Ok(received) => received,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if src != SocketAddr::V6(control) {
            continue;
        }
        let &[version, status] = &buf[..len] else {
            continue;
        };
        if version != simulator::VERSION {
            return Err(UdpError::SimulatorRejected {
                status: Status::Invalid.as_u8(),
            });
        }
        return match Status::from_u8(status) {
            Some(Status::Ok) => {
                tracing::debug!(
                    num_devices = geometry.num_devices(),
                    "the simulator took the geometry"
                );
                Ok(())
            }
            _ => Err(UdpError::SimulatorRejected { status }),
        };
    }
}

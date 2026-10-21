use std::io;
use std::net::{Ipv6Addr, SocketAddrV6, UdpSocket};
use std::time::{Duration, Instant};

use autd3_rs_core::geometry::Geometry;
use thiserror::Error;

use crate::message::{Status, VERSION, control_port, encode_geometry};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SendGeometryError {
    #[error(
        "the simulator port must be within 1..=65534 (its control port is port + 1), but {0} was given"
    )]
    InvalidPort(u16),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("no simulator listens on {0}")]
    NotRunning(SocketAddrV6),
    #[error("the simulator did not answer the geometry at {0} in time")]
    NoResponse(SocketAddrV6),
    #[error("the simulator speaks control version {got}, but this sender speaks version {VERSION}")]
    UnsupportedVersion { got: u8 },
    #[error("the simulator refused the geometry with status {status:#04x}")]
    Rejected { status: u8 },
}

pub fn send_geometry(
    port: u16,
    geometry: &Geometry,
    timeout: Duration,
) -> Result<(), SendGeometryError> {
    let control = control_port(port).ok_or(SendGeometryError::InvalidPort(port))?;
    let control = SocketAddrV6::new(Ipv6Addr::LOCALHOST, control, 0, 0);
    let deadline = Instant::now().checked_add(timeout);
    let socket = UdpSocket::bind((Ipv6Addr::LOCALHOST, 0))?;
    socket.connect(control)?;
    socket.send(&encode_geometry(geometry))?;
    let mut buf = [0u8; 16];
    loop {
        let remaining = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|remaining| remaining.is_zero()) {
            return Err(SendGeometryError::NoResponse(control));
        }
        socket.set_read_timeout(remaining)?;
        let len = match socket.recv(&mut buf) {
            Ok(len) => len,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionReset
                ) =>
            {
                return Err(SendGeometryError::NotRunning(control));
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let Some(answer) = parse_reply(&buf[..len]) else {
            continue;
        };
        if answer.is_ok() {
            tracing::debug!(
                num_devices = geometry.num_devices(),
                "the simulator took the geometry"
            );
        }
        return answer;
    }
}

fn parse_reply(reply: &[u8]) -> Option<Result<(), SendGeometryError>> {
    let &[version, status] = reply else {
        return None;
    };
    if version != VERSION {
        return Some(Err(SendGeometryError::UnsupportedVersion { got: version }));
    }
    Some(match Status::from_u8(status) {
        Some(Status::Ok) => Ok(()),
        _ => Err(SendGeometryError::Rejected { status }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reply_is_read_as_its_version_and_status() {
        assert!(parse_reply(&[VERSION]).is_none());
        assert!(parse_reply(&[VERSION, 0, 0]).is_none());
        assert!(matches!(
            parse_reply(&[VERSION, Status::Ok.as_u8()]),
            Some(Ok(()))
        ));
        assert!(matches!(
            parse_reply(&[VERSION.wrapping_add(1), Status::Ok.as_u8()]),
            Some(Err(SendGeometryError::UnsupportedVersion { got })) if got == VERSION.wrapping_add(1)
        ));
        let rejected = Status::Invalid.as_u8();
        assert!(matches!(
            parse_reply(&[VERSION, rejected]),
            Some(Err(SendGeometryError::Rejected { status })) if status == rejected
        ));
    }

    #[test]
    fn a_port_without_room_for_the_control_port_is_refused() {
        let geometry = Geometry::new(vec![autd3_rs_core::geometry::Autd3::default()]);
        for port in [0, u16::MAX] {
            assert!(matches!(
                send_geometry(port, &geometry, Duration::ZERO),
                Err(SendGeometryError::InvalidPort(p)) if p == port
            ));
        }
    }
}

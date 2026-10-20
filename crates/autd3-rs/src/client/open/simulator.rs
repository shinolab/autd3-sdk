use std::net::SocketAddrV6;
use std::time::{Duration, Instant};

use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::simulator;

use crate::udp::{SimulatorControlSocket, UdpError, parse_simulator_reply};

const READ_TIMEOUT_WITHOUT_DEADLINE: Duration = Duration::from_secs(1);

pub(super) fn send_geometry(
    control: SocketAddrV6,
    geometry: &Geometry,
    deadline: Option<Instant>,
) -> Result<(), UdpError> {
    let socket = SimulatorControlSocket::open(control)?;
    socket.send(&simulator::encode_geometry(geometry))?;
    let mut buf = [0u8; 16];
    loop {
        let remaining = match deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => READ_TIMEOUT_WITHOUT_DEADLINE,
        };
        if remaining.is_zero() {
            return Err(UdpError::SimulatorControlNoResponse(control));
        }
        let Some(len) = socket.recv(&mut buf, remaining)? else {
            continue;
        };
        let Some(answer) = parse_simulator_reply(&buf[..len]) else {
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

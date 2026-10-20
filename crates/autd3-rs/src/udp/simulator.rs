use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::Duration;

use autd3_rs_core::simulator;

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

pub(crate) struct SimulatorControlSocket {
    socket: UdpSocket,
    control: SocketAddrV6,
}

impl SimulatorControlSocket {
    pub(crate) fn open(control: SocketAddrV6) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind((Ipv6Addr::LOCALHOST, 0))?,
            control,
        })
    }

    pub(crate) fn send(&self, datagram: &[u8]) -> io::Result<()> {
        self.socket.send_to(datagram, self.control).map(|_| ())
    }

    pub(crate) fn recv(&self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        self.socket.set_read_timeout(Some(timeout))?;
        match self.socket.recv_from(buf) {
            Ok((len, src)) if src == SocketAddr::V6(self.control) => Ok(Some(len)),
            Ok(_) => Ok(None),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_simulator_at_a_port_controls_through_the_next_port() {
        let simulator = SimulatorAddrs::at(50000).unwrap();
        assert_eq!(simulator.data.port(), 50000);
        assert_eq!(simulator.control.port(), 50001);
        assert_eq!(SimulatorAddrs::DEFAULT.data.port(), 44336);
        assert_eq!(SimulatorAddrs::DEFAULT.control.port(), 44337);
    }
}

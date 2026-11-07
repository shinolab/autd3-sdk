use std::io;
use std::net::SocketAddr;
use std::time::Instant;

use nix::sys::socket::{ControlMessageOwned, sockopt};
use nix::sys::time::TimeValLike;
use socket2::Socket;
use tokio::net::UdpSocket;

use super::unix::recv_with_kernel_stamp;

pub(in crate::udp) struct Stamping;

impl Stamping {
    pub(in crate::udp) fn open(socket: &Socket) -> io::Result<Self> {
        nix::sys::socket::setsockopt(socket, sockopt::ReceiveTimestampns, &true)?;
        Ok(Self)
    }

    pub(in crate::udp) fn try_recv(
        _stamping: &Self,
        socket: &UdpSocket,
        buf: &mut [u8],
        opened_at: Instant,
    ) -> io::Result<(usize, SocketAddr, Instant)> {
        recv_with_kernel_stamp(socket, buf, opened_at, kernel_arrival_ns)
    }
}

fn kernel_arrival_ns(cmsg: &ControlMessageOwned) -> Option<i64> {
    if let &ControlMessageOwned::ScmTimestampns(arrival) = cmsg {
        Some(arrival.num_nanoseconds())
    } else {
        None
    }
}

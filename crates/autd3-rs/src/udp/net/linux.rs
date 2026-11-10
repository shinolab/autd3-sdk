use std::io;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::socket::{ControlMessageOwned, MsgFlags, SockaddrIn6, recvmsg, sockopt};
use nix::sys::time::TimeValLike;
use socket2::Socket;
use tokio::net::UdpSocket;

use super::unix::recv_with_kernel_stamp;

pub(in crate::udp) struct Stamping;

impl Stamping {
    pub(in crate::udp) fn open(socket: &Socket) -> io::Result<Self> {
        nix::sys::socket::setsockopt(socket, sockopt::ReceiveTimestampns, &true)?;
        nix::sys::socket::setsockopt(socket, sockopt::Ipv6RecvErr, &true)?;
        Ok(Self)
    }

    pub(in crate::udp) fn try_recv(
        _stamping: &Self,
        socket: &UdpSocket,
        buf: &mut [u8],
        opened_at: Instant,
    ) -> io::Result<(usize, SocketAddr, Instant)> {
        match recv_with_kernel_stamp(socket, buf, opened_at, kernel_arrival_ns) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Err(take_queued_errors(socket)?.unwrap_or(e))
            }
            Err(e) => {
                take_queued_errors(socket)?;
                Err(e)
            }
            received => received,
        }
    }
}

fn take_queued_errors(socket: &UdpSocket) -> io::Result<Option<io::Error>> {
    let mut first = None;
    loop {
        let mut cmsg = nix::cmsg_space!(
            nix::libc::sock_extended_err,
            nix::libc::sockaddr_in6,
            nix::sys::time::TimeSpec
        );
        let mut iov = [io::IoSliceMut::new(&mut [])];
        match recvmsg::<SockaddrIn6>(
            socket.as_raw_fd(),
            &mut iov,
            Some(&mut cmsg),
            MsgFlags::MSG_ERRQUEUE,
        ) {
            Ok(msg) => {
                if first.is_none() {
                    first = msg.cmsgs()?.find_map(|cmsg| {
                        if let ControlMessageOwned::Ipv6RecvErr(err, _) = cmsg {
                            i32::try_from(err.ee_errno)
                                .ok()
                                .map(io::Error::from_raw_os_error)
                        } else {
                            None
                        }
                    });
                }
            }
            Err(Errno::EAGAIN) => return Ok(first),
            Err(e) => return Err(e.into()),
        }
    }
}

fn kernel_arrival_ns(cmsg: &ControlMessageOwned) -> Option<i64> {
    if let &ControlMessageOwned::ScmTimestampns(arrival) = cmsg {
        Some(arrival.num_nanoseconds())
    } else {
        None
    }
}

use std::io;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::time::Instant;

use nix::sys::socket::{ControlMessageOwned, MsgFlags, SockaddrIn6, recvmsg};
use nix::sys::time::TimeValLike;
use nix::time::{ClockId, clock_gettime};
use tokio::net::UdpSocket;

use super::arrival_instant;

pub(super) fn recv_with_kernel_stamp(
    socket: &UdpSocket,
    buf: &mut [u8],
    opened_at: Instant,
    kernel_arrival_ns: fn(&ControlMessageOwned) -> Option<i64>,
) -> io::Result<(usize, SocketAddr, Instant)> {
    let mut cmsg = nix::cmsg_space!(nix::sys::time::TimeSpec);
    socket.try_io(tokio::io::Interest::READABLE, || {
        let mut iov = [io::IoSliceMut::new(buf)];
        let msg = recvmsg::<SockaddrIn6>(
            socket.as_raw_fd(),
            &mut iov,
            Some(&mut cmsg),
            MsgFlags::empty(),
        )?;
        let now = Instant::now();
        let realtime_now = clock_gettime(ClockId::CLOCK_REALTIME)?.num_nanoseconds();
        let queued_ns = msg
            .cmsgs()?
            .find_map(|cmsg| kernel_arrival_ns(&cmsg))
            .map(|arrival| realtime_now.saturating_sub(arrival));
        let src = msg.address.ok_or(io::ErrorKind::AddrNotAvailable)?;
        Ok((
            msg.bytes,
            SocketAddr::V6(src.into()),
            arrival_instant(now, opened_at, queued_ns),
        ))
    })
}

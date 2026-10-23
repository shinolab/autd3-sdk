use std::io::{self, IoSlice};
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Header, Kind};
use socket2::{Domain, Protocol, Socket, Type};
use zerocopy::{FromBytes, IntoBytes};

pub(crate) const RECV_BUFFER_BYTES: usize = 2048;

pub(crate) enum Polled<R> {
    Empty,
    Wake,
    Device(R),
}

pub(crate) fn same_endpoint(a: SocketAddrV6, b: SocketAddrV6) -> bool {
    a.ip() == b.ip() && a.port() == b.port()
}

#[derive(Clone, Copy)]
enum Interest {
    Readable,
    Writable,
}

pub(crate) struct Channel {
    socket: UdpSocket,
    waker_socket: UdpSocket,
    all_devices_dst: SocketAddrV6,
    device_addrs: Vec<SocketAddrV6>,
    waker_socket_port: u16,
}

#[cfg(unix)]
mod sys {
    use std::ffi::c_int;
    use std::io::{self, IoSlice};
    use std::net::{SocketAddr, SocketAddrV6, UdpSocket};
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    use socket2::SockRef;

    use super::Interest;

    pub(super) fn send_now(
        socket: &UdpSocket,
        datagram: &[IoSlice<'_>],
        dst: SocketAddrV6,
    ) -> io::Result<usize> {
        SockRef::from(socket).send_to_vectored_with_flags(
            datagram,
            &SocketAddr::V6(dst).into(),
            libc::MSG_DONTWAIT,
        )
    }

    pub(super) fn wait(
        socket: &impl AsRawFd,
        interest: Interest,
        timeout: Option<Duration>,
    ) -> io::Result<bool> {
        let mut fd = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: match interest {
                Interest::Readable => libc::POLLIN,
                Interest::Writable => libc::POLLOUT,
            },
            revents: 0,
        };
        let millis = timeout.map_or(-1, |timeout| {
            c_int::try_from(timeout.as_micros().div_ceil(1000)).unwrap_or(c_int::MAX)
        });
        loop {
            match unsafe { libc::poll(&raw mut fd, 1, millis) } {
                0 => return Ok(false),
                ready if ready > 0 => return Ok(true),
                _ => {
                    let e = io::Error::last_os_error();
                    if e.kind() != io::ErrorKind::Interrupted {
                        return Err(e);
                    }
                }
            }
        }
    }

    pub(super) fn is_oversized(_e: &io::Error) -> bool {
        false
    }
}

#[cfg(windows)]
mod sys {
    use std::io::{self, IoSlice};
    use std::net::{SocketAddr, SocketAddrV6, UdpSocket};
    use std::os::windows::io::AsRawSocket;
    use std::time::Duration;

    use socket2::SockRef;
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, POLLWRNORM, SOCKET_ERROR, WSAEMSGSIZE, WSAPOLLFD, WSAPoll,
    };

    use super::Interest;

    pub(super) fn send_now(
        socket: &UdpSocket,
        datagram: &[IoSlice<'_>],
        dst: SocketAddrV6,
    ) -> io::Result<usize> {
        SockRef::from(socket).send_to_vectored(datagram, &SocketAddr::V6(dst).into())
    }

    pub(super) fn wait(
        socket: &impl AsRawSocket,
        interest: Interest,
        timeout: Option<Duration>,
    ) -> io::Result<bool> {
        let mut fd = WSAPOLLFD {
            fd: usize::try_from(socket.as_raw_socket()).expect("a socket handle fits in a pointer"),
            events: match interest {
                Interest::Readable => POLLRDNORM,
                Interest::Writable => POLLWRNORM,
            },
            revents: 0,
        };
        let millis = timeout.map_or(-1, |timeout| {
            i32::try_from(timeout.as_micros().div_ceil(1000)).unwrap_or(i32::MAX)
        });
        loop {
            match unsafe { WSAPoll(&raw mut fd, 1, millis) } {
                0 => return Ok(false),
                SOCKET_ERROR => {
                    let e = io::Error::last_os_error();
                    if e.kind() != io::ErrorKind::Interrupted {
                        return Err(e);
                    }
                }
                _ => return Ok(true),
            }
        }
    }

    pub(super) fn is_oversized(e: &io::Error) -> bool {
        e.raw_os_error() == Some(WSAEMSGSIZE)
    }
}

fn is_undeliverable(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::HostUnreachable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::NetworkDown
            | io::ErrorKind::AddrNotAvailable
    )
}

fn is_transient(e: &io::Error) -> bool {
    is_undeliverable(e) || e.kind() == io::ErrorKind::Interrupted
}

fn send_until_accepted(
    retry_full_socket_until: Option<Instant>,
    mut attempt: impl FnMut() -> io::Result<usize>,
    mut wait_writable: impl FnMut(Duration) -> io::Result<()>,
) -> io::Result<()> {
    loop {
        match attempt() {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let remaining = retry_full_socket_until
                    .map(|retry_full_socket_until| {
                        retry_full_socket_until.saturating_duration_since(Instant::now())
                    })
                    .unwrap_or_default();
                if remaining.is_zero() {
                    return Err(e);
                }
                wait_writable(remaining)?;
            }
            Err(e) => return Err(e),
        }
    }
}

impl Channel {
    pub(crate) fn open(
        all_devices_dst: SocketAddrV6,
        scope_id: Option<u32>,
        send_buffer_bytes: Option<NonZeroUsize>,
    ) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(true)?;
        if let Some(scope_id) = scope_id {
            socket.set_multicast_if_v6(scope_id)?;
        }
        socket.set_multicast_hops_v6(1)?;
        socket.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())?;
        socket.set_nonblocking(true)?;
        if let Some(bytes) = send_buffer_bytes {
            socket.set_send_buffer_size(bytes.get())?;
            tracing::debug!(
                requested = bytes,
                granted = socket.send_buffer_size()?,
                "send buffer set"
            );
        }
        let socket: UdpSocket = socket.into();
        let waker_socket = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0))?;
        waker_socket.set_nonblocking(true)?;
        waker_socket.connect(SocketAddrV6::new(
            Ipv6Addr::LOCALHOST,
            socket.local_addr()?.port(),
            0,
            0,
        ))?;
        let waker_socket_port = waker_socket.local_addr()?.port();
        Ok(Self {
            socket,
            waker_socket,
            all_devices_dst,
            device_addrs: Vec::new(),
            waker_socket_port,
        })
    }

    pub(crate) fn attach_devices(&mut self, device_addrs: Vec<SocketAddrV6>) {
        self.device_addrs = device_addrs;
    }

    #[must_use]
    pub(crate) fn num_devices(&self) -> usize {
        self.device_addrs.len()
    }

    pub(crate) fn all_devices_dst(&self) -> SocketAddrV6 {
        self.all_devices_dst
    }

    fn is_from_waker(&self, src: SocketAddrV6) -> bool {
        src.ip().is_loopback() && src.port() == self.waker_socket_port
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        self.waker_socket.send(&[]).map(|_| ())
    }

    pub(crate) fn send_to_device(
        &self,
        device: usize,
        kind: Kind,
        msg_id: u16,
        body: [&[u8]; 2],
        retry_full_socket_until: Option<Instant>,
    ) -> io::Result<Instant> {
        self.send(
            self.device_addrs[device],
            kind,
            msg_id,
            body,
            retry_full_socket_until,
        )
    }

    pub(crate) fn send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: [&[u8]; 2],
        retry_full_socket_until: Option<Instant>,
    ) -> io::Result<Instant> {
        let header = Header::new(kind, msg_id);
        let datagram = [
            IoSlice::new(header.as_bytes()),
            IoSlice::new(body[0]),
            IoSlice::new(body[1]),
        ];
        let mut attempted_at = Instant::now();
        let sent = send_until_accepted(
            retry_full_socket_until,
            || {
                attempted_at = Instant::now();
                sys::send_now(&self.socket, &datagram, dst)
            },
            |remaining| sys::wait(&self.socket, Interest::Writable, Some(remaining)).map(|_| ()),
        );
        match sent {
            Ok(()) => Ok(attempted_at),
            Err(e) if is_undeliverable(&e) => {
                tracing::debug!(%dst, ?kind, "sending a datagram failed: {e}");
                Ok(attempted_at)
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) fn try_recv_from_device<R>(
        &self,
        mut parse: impl FnMut(usize, Header, &[u8]) -> Option<R>,
    ) -> io::Result<Polled<R>> {
        let mut buf = [0; RECV_BUFFER_BYTES];
        let deadline = Some(Instant::now());
        while let Some((src, datagram)) = self.recv_datagram(&mut buf, deadline)? {
            if self.is_from_waker(src) {
                return Ok(Polled::Wake);
            }
            let Some(device) = self
                .device_addrs
                .iter()
                .position(|&addr| same_endpoint(addr, src))
            else {
                continue;
            };
            let Ok((header, body)) = Header::read_from_prefix(datagram) else {
                continue;
            };
            if let Some(parsed) = parse(device, header, body) {
                return Ok(Polled::Device(parsed));
            }
        }
        Ok(Polled::Empty)
    }

    pub(crate) fn recv_datagram<'a>(
        &self,
        buf: &'a mut [u8; RECV_BUFFER_BYTES],
        deadline: Option<Instant>,
    ) -> io::Result<Option<(SocketAddrV6, &'a [u8])>> {
        let (len, src) = loop {
            match self.socket.recv_from(&mut buf[..]) {
                Ok((len, src)) if len >= RECV_BUFFER_BYTES => {
                    tracing::debug!(%src, "dropped a datagram that does not fit the receive buffer");
                }
                Ok((len, SocketAddr::V6(src))) => break (len, src),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let remaining =
                        deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
                    if remaining.is_some_and(|remaining| remaining.is_zero())
                        || !sys::wait(&self.socket, Interest::Readable, remaining)?
                    {
                        return Ok(None);
                    }
                }
                Err(e) if is_transient(&e) => {}
                Err(e) if sys::is_oversized(&e) => {
                    tracing::debug!("dropped a datagram that does not fit the receive buffer");
                }
                Err(e) => return Err(e),
            }
        };
        Ok(Some((src, &buf[..len])))
    }

    pub(crate) fn wait_readable(&self, deadline: Instant) -> io::Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        sys::wait(&self.socket, Interest::Readable, Some(remaining)).map(|_| ())
    }

    pub(crate) fn socket(&self) -> &UdpSocket {
        &self.socket
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn an_interrupted_send_is_attempted_again_until_the_socket_takes_it() {
        let attempts = Cell::new(0);
        let result = send_until_accepted(
            None,
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 3 {
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                } else {
                    Ok(0)
                }
            },
            |_| panic!("an interrupted send does not wait for the socket"),
        );
        assert!(result.is_ok());
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn an_interrupted_send_keeps_the_error_of_the_attempt_that_follows() {
        let attempts = Cell::new(0);
        let result = send_until_accepted(
            None,
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::from(if attempts.get() < 2 {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::PermissionDenied
                }))
            },
            |_| Ok(()),
        );
        assert_eq!(
            result.map_err(|e| e.kind()),
            Err(io::ErrorKind::PermissionDenied)
        );
        assert_eq!(attempts.get(), 2);
    }

    #[test]
    fn a_full_socket_without_a_deadline_fails_at_the_first_attempt() {
        let attempts = Cell::new(0);
        let result = send_until_accepted(
            None,
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            },
            |_| panic!("no deadline leaves nothing to wait for"),
        );
        assert_eq!(result.map_err(|e| e.kind()), Err(io::ErrorKind::WouldBlock));
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn an_interruption_is_not_counted_as_a_sent_datagram() {
        let interrupted = io::Error::from(io::ErrorKind::Interrupted);
        assert!(!is_undeliverable(&interrupted));
        assert!(is_transient(&interrupted));
        assert!(is_undeliverable(&io::Error::from(
            io::ErrorKind::HostUnreachable
        )));
    }
}

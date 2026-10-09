use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::num::NonZeroUsize;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{ALL_NODES, Header, Kind, PORT, PROTOCOL_VERSION};
use autd3_rs_core::FRAME_BYTES_MAX;
use socket2::{Domain, Protocol, Socket, Type};
use zerocopy::{FromBytes, IntoBytes};

use super::error::UdpError;
use super::pacer::Pacer;

const RECV_BUFFER_BYTES: usize = 2048;

#[derive(Clone, Debug)]
pub(crate) struct Response {
    pub(crate) src: SocketAddrV6,
    pub(crate) status: u8,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct Datagram<'a> {
    pub(crate) src: SocketAddrV6,
    pub(crate) header: Header,
    pub(crate) body: &'a [u8],
}

struct Tx {
    msg_id: u16,
    pacer: Option<Pacer>,
}

struct Rx {
    nonblocking: bool,
    buf: Box<[u8; RECV_BUFFER_BYTES]>,
}

pub(crate) struct Channel {
    socket: UdpSocket,
    waker: UdpSocket,
    group: SocketAddrV6,
    local_port: u16,
    waker_port: u16,
    tx: Mutex<Tx>,
    rx: Mutex<Rx>,
}

#[cfg(unix)]
mod sys {
    use std::ffi::c_int;
    use std::io;
    use std::net::{SocketAddr, SocketAddrV6, UdpSocket};
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    use socket2::SockRef;

    pub(super) fn send_now(
        socket: &UdpSocket,
        datagram: &[u8],
        dst: SocketAddrV6,
    ) -> io::Result<usize> {
        SockRef::from(socket).send_to_with_flags(
            datagram,
            &SocketAddr::V6(dst).into(),
            libc::MSG_DONTWAIT,
        )
    }

    pub(super) fn wait_writable(socket: &UdpSocket, timeout: Duration) -> io::Result<()> {
        let mut fd = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        let millis = c_int::try_from(timeout.as_micros().div_ceil(1000)).unwrap_or(c_int::MAX);
        if unsafe { libc::poll(&raw mut fd, 1, millis) } < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
        Ok(())
    }

    pub(super) fn is_oversized(_e: &io::Error) -> bool {
        false
    }
}

#[cfg(not(unix))]
mod sys {
    use std::io;
    use std::net::{SocketAddrV6, UdpSocket};
    use std::time::Duration;

    const RETRY_INTERVAL: Duration = Duration::from_millis(1);
    #[cfg(windows)]
    const WSAEMSGSIZE: i32 = 10040;

    pub(super) fn send_now(
        socket: &UdpSocket,
        datagram: &[u8],
        dst: SocketAddrV6,
    ) -> io::Result<usize> {
        socket.send_to(datagram, dst)
    }

    #[allow(clippy::unnecessary_wraps)]
    pub(super) fn wait_writable(_socket: &UdpSocket, timeout: Duration) -> io::Result<()> {
        std::thread::sleep(timeout.min(RETRY_INTERVAL));
        Ok(())
    }

    #[cfg(windows)]
    pub(super) fn is_oversized(e: &io::Error) -> bool {
        e.raw_os_error() == Some(WSAEMSGSIZE)
    }

    #[cfg(not(windows))]
    pub(super) fn is_oversized(_e: &io::Error) -> bool {
        false
    }
}

pub(crate) fn all_nodes(scope: u32) -> SocketAddrV6 {
    SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, scope)
}

pub(super) fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn is_transient(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::HostUnreachable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::NetworkDown
            | io::ErrorKind::AddrNotAvailable
            | io::ErrorKind::Interrupted
    )
}

impl Channel {
    pub(crate) fn open(
        group: SocketAddrV6,
        scope: Option<u32>,
        send_rate_limit: Option<f32>,
        send_buffer: Option<NonZeroUsize>,
    ) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(true)?;
        if let Some(scope) = scope {
            socket.set_multicast_if_v6(scope)?;
        }
        socket.set_multicast_hops_v6(1)?;
        socket.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())?;
        if let Some(bytes) = send_buffer {
            socket.set_send_buffer_size(bytes.get())?;
            tracing::debug!(
                requested = bytes,
                granted = socket.send_buffer_size()?,
                "send buffer set"
            );
        }
        let socket: UdpSocket = socket.into();
        let local_port = socket.local_addr()?.port();
        let waker = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0))?;
        waker.set_nonblocking(true)?;
        let waker_port = waker.local_addr()?.port();
        Ok(Self {
            socket,
            waker,
            group,
            local_port,
            waker_port,
            tx: Mutex::new(Tx {
                msg_id: 0,
                pacer: send_rate_limit.map(Pacer::new),
            }),
            rx: Mutex::new(Rx {
                nonblocking: false,
                buf: Box::new([0; RECV_BUFFER_BYTES]),
            }),
        })
    }

    fn tx(&self) -> MutexGuard<'_, Tx> {
        self.tx.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn rx(&self) -> MutexGuard<'_, Rx> {
        self.rx.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn group(&self) -> SocketAddrV6 {
        self.group
    }

    pub(crate) fn is_wake(&self, src: SocketAddrV6) -> bool {
        src.ip().is_loopback() && src.port() == self.waker_port
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        let dst = SocketAddrV6::new(Ipv6Addr::LOCALHOST, self.local_port, 0, 0);
        self.waker.send_to(&[], dst).map(|_| ())
    }

    fn arm(&self, rx: &mut Rx, deadline: Option<Instant>) -> io::Result<()> {
        let remaining = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|remaining| remaining.is_zero()) {
            if !rx.nonblocking {
                self.socket.set_nonblocking(true)?;
                rx.nonblocking = true;
            }
            return Ok(());
        }
        if rx.nonblocking {
            self.socket.set_nonblocking(false)?;
            rx.nonblocking = false;
        }
        self.socket.set_read_timeout(remaining)
    }

    pub(crate) fn next_msg_id(&self) -> u16 {
        let mut tx = self.tx();
        tx.msg_id = tx.msg_id.wrapping_add(1);
        tx.msg_id
    }

    pub(crate) fn send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: &[u8],
        give_up: Option<Instant>,
    ) -> io::Result<()> {
        self.send_with(dst, kind, msg_id, body, give_up, || {})
    }

    pub(crate) fn send_with(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: &[u8],
        give_up: Option<Instant>,
        before_attempt: impl FnMut(),
    ) -> io::Result<()> {
        let mut datagram = [0u8; size_of::<Header>() + FRAME_BYTES_MAX];
        let len = size_of::<Header>() + body.len();
        datagram[..size_of::<Header>()].copy_from_slice(Header::new(kind, msg_id).as_bytes());
        datagram[size_of::<Header>()..len].copy_from_slice(body);
        let wait = self
            .tx()
            .pacer
            .as_mut()
            .map_or(Duration::ZERO, |pacer| pacer.reserve(Instant::now(), len));
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        match self.transmit(&datagram[..len], dst, give_up, before_attempt) {
            Ok(()) => Ok(()),
            Err(e) if is_transient(&e) => {
                tracing::debug!(%dst, ?kind, "sending a datagram failed: {e}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn send_now(&self, datagram: &[u8], dst: SocketAddrV6) -> io::Result<usize> {
        sys::send_now(&self.socket, datagram, dst)
    }

    fn transmit(
        &self,
        datagram: &[u8],
        dst: SocketAddrV6,
        give_up: Option<Instant>,
        mut before_attempt: impl FnMut(),
    ) -> io::Result<()> {
        loop {
            before_attempt();
            match self.send_now(datagram, dst) {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let remaining = give_up
                        .map(|give_up| give_up.saturating_duration_since(Instant::now()))
                        .unwrap_or_default();
                    if remaining.is_zero() {
                        return Err(e);
                    }
                    sys::wait_writable(&self.socket, remaining)?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub(crate) fn request(&self, dst: SocketAddrV6, kind: Kind, body: &[u8]) -> io::Result<u16> {
        let msg_id = self.next_msg_id();
        self.send(dst, kind, msg_id, body, None)?;
        Ok(msg_id)
    }

    pub(crate) fn recv<R>(
        &self,
        deadline: Option<Instant>,
        parse: impl FnOnce(&Datagram<'_>) -> R,
    ) -> io::Result<Option<R>> {
        let mut rx = self.rx();
        let (len, src) = loop {
            self.arm(&mut rx, deadline)?;
            match self.socket.recv_from(&mut rx.buf[..]) {
                Ok((len, src)) if len >= RECV_BUFFER_BYTES => {
                    tracing::debug!(%src, "dropped a datagram that does not fit the receive buffer");
                }
                Ok((len, SocketAddr::V6(src))) if len >= size_of::<Header>() => break (len, src),
                Ok((_, SocketAddr::V6(src))) if self.is_wake(src) => {
                    let header = Header::new(Kind::Heartbeat, 0);
                    return Ok(Some(parse(&Datagram {
                        src,
                        header,
                        body: &[],
                    })));
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => return Ok(None),
                Err(e) if is_transient(&e) => {}
                Err(e) if sys::is_oversized(&e) => {
                    tracing::debug!("dropped a datagram that does not fit the receive buffer");
                }
                Err(e) => return Err(e),
            }
        };
        let datagram = &rx.buf[..len];
        let (header, body) = Header::read_from_prefix(datagram)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(Some(parse(&Datagram { src, header, body })))
    }

    pub(crate) fn wait_readable(&self, deadline: Instant) -> io::Result<()> {
        let mut rx = self.rx();
        self.arm(&mut rx, Some(deadline))?;
        match self.socket.peek_from(&mut rx.buf[..]) {
            Ok(_) => Ok(()),
            Err(e) if is_timeout(&e) || is_transient(&e) || sys::is_oversized(&e) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    pub(crate) fn collect(
        &self,
        kind: Kind,
        msg_id: u16,
        window: Duration,
        enough: Option<usize>,
    ) -> Result<Vec<Response>, UdpError> {
        let deadline = Instant::now().checked_add(window);
        let mut responses = Vec::new();
        while enough.is_none_or(|n| responses.len() < n) {
            let Some(parsed) =
                self.recv(deadline, |datagram| parse_response(datagram, kind, msg_id))?
            else {
                break;
            };
            if let Some(response) = parsed? {
                responses.push(response);
            }
        }
        Ok(responses)
    }

    pub(crate) fn first(
        &self,
        kind: Kind,
        msg_id: u16,
        timeout: Duration,
    ) -> Result<Option<Response>, UdpError> {
        Ok(self.collect(kind, msg_id, timeout, Some(1))?.pop())
    }

    pub(crate) fn exchange(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
        timeout: Duration,
    ) -> Result<Option<Response>, UdpError> {
        let msg_id = self.request(dst, kind, body)?;
        self.first(kind, msg_id, timeout)
    }
}

fn parse_response(
    datagram: &Datagram<'_>,
    kind: Kind,
    msg_id: u16,
) -> Result<Option<Response>, UdpError> {
    if datagram.header.kind != kind.as_u8() || datagram.header.msg_id.get() != msg_id {
        return Ok(None);
    }
    if datagram.header.version != PROTOCOL_VERSION {
        return Err(UdpError::UnsupportedVersion {
            device: datagram.header.version,
            host: PROTOCOL_VERSION,
        });
    }
    let Some((&status, body)) = datagram.body.split_first() else {
        return Ok(None);
    };
    Ok(Some(Response {
        src: datagram.src,
        status,
        body: body.to_vec(),
    }))
}

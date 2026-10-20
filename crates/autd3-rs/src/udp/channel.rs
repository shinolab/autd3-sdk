use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::num::NonZeroUsize;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Header, Kind, PROTOCOL_VERSION};
use autd3_rs_core::FRAME_BYTES_MAX;
use socket2::{Domain, Protocol, Socket, Type};
use zerocopy::{FromBytes, IntoBytes};

use super::error::UdpError;
use super::pacer::Pacer;

const RECV_BUFFER_BYTES: usize = 2048;

#[derive(Clone, Debug)]
pub(crate) struct ControlResponse {
    pub(crate) src: SocketAddrV6,
    pub(crate) status: u8,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct ReceivedPacket<'a> {
    pub(crate) src: SocketAddrV6,
    pub(crate) header: Header,
    pub(crate) body: &'a [u8],
}

struct SendState {
    last_request_msg_id: u16,
    pacer: Option<Pacer>,
}

struct RecvState {
    socket_is_nonblocking: bool,
    buf: Box<[u8; RECV_BUFFER_BYTES]>,
}

pub(crate) struct Channel {
    socket: UdpSocket,
    waker_socket: UdpSocket,
    all_devices_dst: SocketAddrV6,
    main_socket_port: u16,
    waker_socket_port: u16,
    send_state: Mutex<SendState>,
    recv_state: Mutex<RecvState>,
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

    pub(super) fn wait_writable(socket: &impl AsRawFd, timeout: Duration) -> io::Result<()> {
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
    pub(crate) fn wait_writable<S: ?Sized>(_socket: &S, timeout: Duration) -> io::Result<()> {
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

pub(super) fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
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
        send_rate_limit_percent: Option<f32>,
        send_buffer_bytes: Option<NonZeroUsize>,
    ) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(true)?;
        if let Some(scope_id) = scope_id {
            socket.set_multicast_if_v6(scope_id)?;
        }
        socket.set_multicast_hops_v6(1)?;
        socket.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())?;
        if let Some(bytes) = send_buffer_bytes {
            socket.set_send_buffer_size(bytes.get())?;
            tracing::debug!(
                requested = bytes,
                granted = socket.send_buffer_size()?,
                "send buffer set"
            );
        }
        let socket: UdpSocket = socket.into();
        let main_socket_port = socket.local_addr()?.port();
        let waker_socket = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0))?;
        waker_socket.set_nonblocking(true)?;
        let waker_socket_port = waker_socket.local_addr()?.port();
        Ok(Self {
            socket,
            waker_socket,
            all_devices_dst,
            main_socket_port,
            waker_socket_port,
            send_state: Mutex::new(SendState {
                last_request_msg_id: 0,
                pacer: send_rate_limit_percent.map(Pacer::new),
            }),
            recv_state: Mutex::new(RecvState {
                socket_is_nonblocking: false,
                buf: Box::new([0; RECV_BUFFER_BYTES]),
            }),
        })
    }

    fn lock_send_state(&self) -> MutexGuard<'_, SendState> {
        self.send_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_recv_state(&self) -> MutexGuard<'_, RecvState> {
        self.recv_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn all_devices_dst(&self) -> SocketAddrV6 {
        self.all_devices_dst
    }

    pub(crate) fn is_from_waker(&self, src: SocketAddrV6) -> bool {
        src.ip().is_loopback() && src.port() == self.waker_socket_port
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        let dst = SocketAddrV6::new(Ipv6Addr::LOCALHOST, self.main_socket_port, 0, 0);
        self.waker_socket.send_to(&[], dst).map(|_| ())
    }

    fn apply_recv_deadline(
        &self,
        recv_state: &mut RecvState,
        deadline: Option<Instant>,
    ) -> io::Result<()> {
        let remaining = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|remaining| remaining.is_zero()) {
            if !recv_state.socket_is_nonblocking {
                self.socket.set_nonblocking(true)?;
                recv_state.socket_is_nonblocking = true;
            }
            return Ok(());
        }
        if recv_state.socket_is_nonblocking {
            self.socket.set_nonblocking(false)?;
            recv_state.socket_is_nonblocking = false;
        }
        self.socket.set_read_timeout(remaining)
    }

    pub(crate) fn allocate_request_msg_id(&self) -> u16 {
        let mut send_state = self.lock_send_state();
        send_state.last_request_msg_id = send_state.last_request_msg_id.wrapping_add(1);
        send_state.last_request_msg_id
    }

    pub(crate) fn send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: &[u8],
        retry_full_socket_until: Option<Instant>,
        before_attempt: impl FnMut(),
    ) -> io::Result<()> {
        let mut datagram = [0u8; size_of::<Header>() + FRAME_BYTES_MAX];
        let len = size_of::<Header>() + body.len();
        datagram[..size_of::<Header>()].copy_from_slice(Header::new(kind, msg_id).as_bytes());
        datagram[size_of::<Header>()..len].copy_from_slice(body);
        let wait = self
            .lock_send_state()
            .pacer
            .as_mut()
            .map_or(Duration::ZERO, |pacer| {
                pacer.reserve_send_slot(Instant::now(), len)
            });
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        match self.send_with_retry(
            &datagram[..len],
            dst,
            retry_full_socket_until,
            before_attempt,
        ) {
            Ok(()) => Ok(()),
            Err(e) if is_undeliverable(&e) => {
                tracing::debug!(%dst, ?kind, "sending a datagram failed: {e}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn send_with_retry(
        &self,
        datagram: &[u8],
        dst: SocketAddrV6,
        retry_full_socket_until: Option<Instant>,
        mut before_attempt: impl FnMut(),
    ) -> io::Result<()> {
        send_until_accepted(
            retry_full_socket_until,
            || {
                before_attempt();
                sys::send_now(&self.socket, datagram, dst)
            },
            |remaining| sys::wait_writable(&self.socket, remaining),
        )
    }

    pub(crate) fn send_request(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> io::Result<u16> {
        let msg_id = self.allocate_request_msg_id();
        self.send(dst, kind, msg_id, body, None, || {})?;
        Ok(msg_id)
    }

    pub(crate) fn recv<R>(
        &self,
        deadline: Option<Instant>,
        classify: impl FnOnce(&ReceivedPacket<'_>) -> R,
    ) -> io::Result<Option<R>> {
        let mut recv_state = self.lock_recv_state();
        let (len, src) = loop {
            self.apply_recv_deadline(&mut recv_state, deadline)?;
            match self.socket.recv_from(&mut recv_state.buf[..]) {
                Ok((len, src)) if len >= RECV_BUFFER_BYTES => {
                    tracing::debug!(%src, "dropped a datagram that does not fit the receive buffer");
                }
                Ok((len, SocketAddr::V6(src))) if len >= size_of::<Header>() => break (len, src),
                Ok((_, SocketAddr::V6(src))) if self.is_from_waker(src) => {
                    let header = Header::new(Kind::Heartbeat, 0);
                    return Ok(Some(classify(&ReceivedPacket {
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
        let datagram = &recv_state.buf[..len];
        let (header, body) = Header::read_from_prefix(datagram)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(Some(classify(&ReceivedPacket { src, header, body })))
    }

    pub(crate) fn wait_readable(&self, deadline: Instant) -> io::Result<()> {
        let mut recv_state = self.lock_recv_state();
        self.apply_recv_deadline(&mut recv_state, Some(deadline))?;
        match self.socket.peek_from(&mut recv_state.buf[..]) {
            Ok(_) => Ok(()),
            Err(e) if is_timeout(&e) || is_transient(&e) || sys::is_oversized(&e) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    pub(crate) fn collect_responses(
        &self,
        kind: Kind,
        msg_id: u16,
        wait: Duration,
        max_responses: Option<usize>,
    ) -> Result<Vec<ControlResponse>, UdpError> {
        let deadline = Instant::now().checked_add(wait);
        let mut responses = Vec::new();
        while max_responses.is_none_or(|n| responses.len() < n) {
            let Some(parsed) =
                self.recv(deadline, |packet| parse_response(packet, kind, msg_id))?
            else {
                break;
            };
            if let Some(response) = parsed? {
                responses.push(response);
            }
        }
        Ok(responses)
    }

    pub(crate) fn request_once(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
        timeout: Duration,
    ) -> Result<Option<ControlResponse>, UdpError> {
        let msg_id = self.send_request(dst, kind, body)?;
        Ok(self
            .collect_responses(kind, msg_id, timeout, Some(1))?
            .pop())
    }
}

fn parse_response(
    packet: &ReceivedPacket<'_>,
    kind: Kind,
    msg_id: u16,
) -> Result<Option<ControlResponse>, UdpError> {
    if packet.header.kind != kind.as_u8() || packet.header.msg_id.get() != msg_id {
        return Ok(None);
    }
    if packet.header.version != PROTOCOL_VERSION {
        return Err(UdpError::UnsupportedVersion {
            device_version: packet.header.version,
            host_version: PROTOCOL_VERSION,
        });
    }
    let Some((&status, body)) = packet.body.split_first() else {
        return Ok(None);
    };
    Ok(Some(ControlResponse {
        src: packet.src,
        status,
        body: body.to_vec(),
    }))
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

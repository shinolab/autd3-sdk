use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{ALL_NODES, HEADER_BYTES, Header, Kind, PORT, PROTOCOL_VERSION};
use socket2::{Domain, Protocol, Socket, Type};
use zerocopy::{FromBytes, IntoBytes};

use super::error::UdpError;

const RECV_BUFFER_BYTES: usize = 2048;
const SEND_BUFFER_BYTES: usize = 1024;

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

pub(crate) struct Channel {
    socket: UdpSocket,
    group: SocketAddrV6,
    msg_id: u16,
    send_buf: Vec<u8>,
    recv_buf: Box<[u8; RECV_BUFFER_BYTES]>,
}

pub(crate) fn all_nodes(scope: u32) -> SocketAddrV6 {
    SocketAddrV6::new(Ipv6Addr::from(ALL_NODES), PORT, 0, scope)
}

fn is_timeout(e: &io::Error) -> bool {
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
    pub(crate) fn open(group: SocketAddrV6, scope: Option<u32>) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(true)?;
        if let Some(scope) = scope {
            socket.set_multicast_if_v6(scope)?;
        }
        socket.set_multicast_hops_v6(1)?;
        socket.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())?;
        Ok(Self {
            socket: socket.into(),
            group,
            msg_id: 0,
            send_buf: Vec::with_capacity(SEND_BUFFER_BYTES),
            recv_buf: Box::new([0; RECV_BUFFER_BYTES]),
        })
    }

    pub(crate) fn group(&self) -> SocketAddrV6 {
        self.group
    }

    pub(crate) fn next_msg_id(&mut self) -> u16 {
        self.msg_id = self.msg_id.wrapping_add(1);
        self.msg_id
    }

    pub(crate) fn send(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: &[u8],
    ) -> io::Result<()> {
        self.send_buf.clear();
        self.send_buf
            .extend_from_slice(Header::new(kind, msg_id).as_bytes());
        self.send_buf.extend_from_slice(body);
        match self.socket.send_to(&self.send_buf, dst) {
            Ok(_) => Ok(()),
            Err(e) if is_transient(&e) => {
                tracing::debug!(%dst, ?kind, "sending a datagram failed: {e}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) fn request(
        &mut self,
        dst: SocketAddrV6,
        kind: Kind,
        body: &[u8],
    ) -> io::Result<u16> {
        let msg_id = self.next_msg_id();
        self.send(dst, kind, msg_id, body)?;
        Ok(msg_id)
    }

    pub(crate) fn recv(&mut self, deadline: Instant) -> io::Result<Option<Datagram<'_>>> {
        let (len, src) = loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            self.socket.set_read_timeout(Some(deadline - now))?;
            match self.socket.recv_from(&mut self.recv_buf[..]) {
                Ok((len, SocketAddr::V6(src))) if len >= HEADER_BYTES => break (len, src),
                Ok(_) => {}
                Err(e) if is_timeout(&e) => return Ok(None),
                Err(e) if is_transient(&e) => {}
                Err(e) => return Err(e),
            }
        };
        let datagram = &self.recv_buf[..len];
        let (header, body) = Header::read_from_prefix(datagram)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(Some(Datagram { src, header, body }))
    }

    pub(crate) fn collect(
        &mut self,
        kind: Kind,
        msg_id: u16,
        window: Duration,
        enough: Option<usize>,
    ) -> Result<Vec<Response>, UdpError> {
        let deadline = Instant::now() + window;
        let mut responses = Vec::new();
        while enough.is_none_or(|n| responses.len() < n) {
            let Some(datagram) = self.recv(deadline)? else {
                break;
            };
            if let Some(response) = parse_response(&datagram, kind, msg_id)? {
                responses.push(response);
            }
        }
        Ok(responses)
    }

    pub(crate) fn first(
        &mut self,
        kind: Kind,
        msg_id: u16,
        timeout: Duration,
    ) -> Result<Option<Response>, UdpError> {
        Ok(self.collect(kind, msg_id, timeout, Some(1))?.pop())
    }

    pub(crate) fn exchange(
        &mut self,
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

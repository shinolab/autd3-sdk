use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::num::NonZeroUsize;
use std::time::Instant;

use autd3_cpu_wire::FRAME_BYTES_MAX;
use autd3_cpu_wire::udp::{Header, Kind};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::timeout_at;
use zerocopy::{FromBytes, IntoBytes};

use super::DeviceReply;

pub(crate) const RECV_BUFFER_BYTES: usize = 2048;

pub(crate) fn same_endpoint(a: SocketAddrV6, b: SocketAddrV6) -> bool {
    a.ip() == b.ip() && a.port() == b.port()
}

pub(crate) struct Channel {
    socket: UdpSocket,
    all_devices_dst: SocketAddrV6,
    device_addrs: Vec<SocketAddrV6>,
}

fn is_oversized(e: &io::Error) -> bool {
    cfg!(windows) && e.raw_os_error() == Some(10040)
}

fn accepted_datagram(
    received: io::Result<(usize, SocketAddr)>,
) -> io::Result<Option<(usize, SocketAddrV6)>> {
    match received {
        Ok((len, src)) if len >= RECV_BUFFER_BYTES => {
            tracing::debug!(%src, "dropped a datagram that does not fit the receive buffer");
            Ok(None)
        }
        Ok((len, SocketAddr::V6(src))) => Ok(Some((len, src))),
        Ok(_) => Ok(None),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::HostUnreachable
                    | io::ErrorKind::NetworkUnreachable
                    | io::ErrorKind::NetworkDown
                    | io::ErrorKind::AddrNotAvailable
                    | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(e) if is_oversized(&e) => {
            tracing::debug!("dropped a datagram that does not fit the receive buffer");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

impl Channel {
    pub(crate) async fn open(
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
        let socket: std::net::UdpSocket = socket.into();
        let socket = UdpSocket::from_std(socket)?;
        socket.writable().await?;
        Ok(Self {
            socket,
            all_devices_dst,
            device_addrs: Vec::new(),
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

    pub(crate) async fn send_to_device(
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
        .await
    }

    fn try_send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: [&[u8]; 2],
    ) -> io::Result<Instant> {
        let header = Header::new(kind, msg_id);
        let mut datagram = [0; size_of::<Header>() + FRAME_BYTES_MAX];
        let mut len = 0;
        for part in [header.as_bytes(), body[0], body[1]] {
            datagram[len..len + part.len()].copy_from_slice(part);
            len += part.len();
        }
        loop {
            let attempted_at = Instant::now();
            match self.socket.try_send_to(&datagram[..len], dst.into()) {
                Ok(_) => return Ok(attempted_at),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    pub(crate) async fn send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: [&[u8]; 2],
        retry_full_socket_until: Option<Instant>,
    ) -> io::Result<Instant> {
        loop {
            match self.try_send(dst, kind, msg_id, body) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let Some(until) =
                        retry_full_socket_until.filter(|&until| until > Instant::now())
                    else {
                        return Err(e);
                    };
                    if let Ok(ready) = timeout_at(until.into(), self.socket.writable()).await {
                        ready?;
                    }
                }
                sent => return sent,
            }
        }
    }

    pub(crate) fn try_recv_from_device(&self) -> io::Result<Option<DeviceReply>> {
        let mut buf = [0; RECV_BUFFER_BYTES];
        loop {
            let (len, src) = match accepted_datagram(self.socket.try_recv_from(&mut buf)) {
                Ok(Some(datagram)) => datagram,
                Ok(None) => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e),
            };
            let Some(device) = self
                .device_addrs
                .iter()
                .position(|&addr| same_endpoint(addr, src))
            else {
                continue;
            };
            let Ok((header, body)) = Header::read_from_prefix(&buf[..len]) else {
                continue;
            };
            if let Some(parsed) = DeviceReply::parse(device, header, body) {
                return Ok(Some(parsed));
            }
        }
    }

    pub(crate) async fn recv_datagram<'a>(
        &self,
        buf: &'a mut [u8; RECV_BUFFER_BYTES],
        deadline: Option<Instant>,
    ) -> io::Result<Option<(SocketAddrV6, &'a [u8])>> {
        let (len, src) = loop {
            match accepted_datagram(self.socket.try_recv_from(&mut buf[..])) {
                Ok(Some(datagram)) => break datagram,
                Ok(None) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => match deadline {
                    Some(deadline) => {
                        match timeout_at(deadline.into(), self.socket.readable()).await {
                            Ok(ready) => ready?,
                            Err(_) => return Ok(None),
                        }
                    }
                    None => self.socket.readable().await?,
                },
                Err(e) => return Err(e),
            }
        };
        Ok(Some((src, &buf[..len])))
    }

    pub(crate) async fn readable(&self) -> io::Result<()> {
        self.socket.readable().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_full_size_datagram_is_the_header_followed_by_both_body_parts() {
        let receiver =
            std::net::UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).unwrap();
        let SocketAddr::V6(dst) = receiver.local_addr().unwrap() else {
            panic!("the receiver is bound to an IPv6 address");
        };
        let channel = Channel::open(dst, None, None).await.unwrap();
        let first = [1; 6];
        let second = [2; FRAME_BYTES_MAX - 6];

        channel
            .send(dst, Kind::Frame, 0x1234, [&first, &second], None)
            .await
            .unwrap();

        let mut buf = [0; RECV_BUFFER_BYTES];
        let (len, _) = receiver.recv_from(&mut buf).unwrap();
        let header = Header::new(Kind::Frame, 0x1234);
        let expected = [header.as_bytes(), &first, &second].concat();
        assert_eq!(&buf[..len], expected);
    }
}

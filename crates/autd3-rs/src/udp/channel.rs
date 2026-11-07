use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::num::NonZeroUsize;
use std::time::Instant;

use autd3_cpu_wire::FRAME_BYTES_MAX;
use autd3_cpu_wire::udp::{Header, Kind};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
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
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    opened_at: Instant,
    #[cfg(windows)]
    wsa_recv_msg: windows_sys::Win32::Networking::WinSock::LPFN_WSARECVMSG,
    #[cfg(windows)]
    qpc_per_second: i64,
}

fn is_oversized(e: &io::Error) -> bool {
    cfg!(windows) && e.raw_os_error() == Some(10040)
}

fn accepted_datagram<T>(
    received: io::Result<(usize, SocketAddr, T)>,
) -> io::Result<Option<(usize, SocketAddrV6, T)>> {
    match received {
        Ok((len, src, _)) if len >= RECV_BUFFER_BYTES => {
            tracing::debug!(%src, "dropped a datagram that does not fit the receive buffer");
            Ok(None)
        }
        Ok((len, SocketAddr::V6(src), extra)) => Ok(Some((len, src, extra))),
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

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn arrival_instant(now: Instant, opened_at: Instant, queued_ns: Option<i64>) -> Instant {
    queued_ns
        .and_then(|queued_ns| u64::try_from(queued_ns).ok())
        .and_then(|queued_ns| now.checked_sub(std::time::Duration::from_nanos(queued_ns)))
        .filter(|&arrival| arrival >= opened_at)
        .unwrap_or(now)
}

#[cfg(target_os = "linux")]
fn kernel_arrival_ns(cmsg: &nix::sys::socket::ControlMessageOwned) -> Option<i64> {
    use nix::sys::time::TimeValLike;

    if let &nix::sys::socket::ControlMessageOwned::ScmTimestampns(arrival) = cmsg {
        Some(arrival.num_nanoseconds())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn kernel_arrival_ns(cmsg: &nix::sys::socket::ControlMessageOwned) -> Option<i64> {
    use nix::sys::time::TimeValLike;

    if let &nix::sys::socket::ControlMessageOwned::ScmTimestamp(arrival) = cmsg {
        Some(arrival.num_nanoseconds())
    } else {
        None
    }
}

#[cfg(any(windows, test))]
fn wsa_rx_timestamp(control: &[u8]) -> Option<u64> {
    let word = size_of::<usize>();
    let header = word + 2 * size_of::<i32>();
    let data_offset = header.next_multiple_of(word);
    let mut rest = control;
    while rest.len() >= header {
        let cmsg_len = usize::from_ne_bytes(rest[..word].try_into().ok()?);
        let level = i32::from_ne_bytes(rest[word..word + 4].try_into().ok()?);
        let kind = i32::from_ne_bytes(rest[word + 4..word + 8].try_into().ok()?);
        if level == 0xFFFF && kind == 0x300A {
            return Some(u64::from_ne_bytes(
                rest.get(data_offset..data_offset + 8)?.try_into().ok()?,
            ));
        }
        rest = rest.get(cmsg_len.max(header).next_multiple_of(word)..)?;
    }
    None
}

#[cfg(any(windows, test))]
fn queued_ns_from_qpc(qpc_now: i64, stamp: u64, qpc_per_second: i64) -> Option<i64> {
    let queued = i128::from(qpc_now) - i128::from(stamp);
    i64::try_from((queued * 1_000_000_000).checked_div(i128::from(qpc_per_second))?).ok()
}

#[cfg(windows)]
fn enable_wsa_rx_timestamps(
    socket: &Socket,
) -> io::Result<windows_sys::Win32::Networking::WinSock::LPFN_WSARECVMSG> {
    use std::os::windows::io::AsRawSocket;

    use windows_sys::Win32::Networking::WinSock::{
        LPFN_WSARECVMSG, SIO_GET_EXTENSION_FUNCTION_POINTER, SIO_TIMESTAMPING, SOCKET,
        TIMESTAMPING_CONFIG, TIMESTAMPING_FLAG_RX, WSAID_WSARECVMSG, WSAIoctl,
    };

    let raw = SOCKET::try_from(socket.as_raw_socket()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let config = TIMESTAMPING_CONFIG {
        Flags: TIMESTAMPING_FLAG_RX,
        TxTimestampsBuffered: 0,
    };
    let mut returned = 0;
    let enabled = unsafe {
        WSAIoctl(
            raw,
            SIO_TIMESTAMPING,
            (&raw const config).cast(),
            u32::try_from(size_of::<TIMESTAMPING_CONFIG>()).unwrap_or(u32::MAX),
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
            None,
        )
    } == 0;
    if !enabled {
        tracing::debug!("receive timestamps are unavailable; replies are stamped when read");
        return Ok(None);
    }
    let guid = WSAID_WSARECVMSG;
    let mut recv_msg: LPFN_WSARECVMSG = None;
    let found = unsafe {
        WSAIoctl(
            raw,
            SIO_GET_EXTENSION_FUNCTION_POINTER,
            (&raw const guid).cast(),
            u32::try_from(size_of_val(&guid)).unwrap_or(u32::MAX),
            (&raw mut recv_msg).cast(),
            u32::try_from(size_of::<LPFN_WSARECVMSG>()).unwrap_or(u32::MAX),
            &raw mut returned,
            std::ptr::null_mut(),
            None,
        )
    } == 0;
    Ok(if found { recv_msg } else { None })
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
        #[cfg(target_os = "linux")]
        nix::sys::socket::setsockopt(
            &socket,
            nix::sys::socket::sockopt::ReceiveTimestampns,
            &true,
        )?;
        #[cfg(target_os = "macos")]
        nix::sys::socket::setsockopt(&socket, nix::sys::socket::sockopt::ReceiveTimestamp, &true)?;
        #[cfg(windows)]
        let wsa_recv_msg = enable_wsa_rx_timestamps(&socket)?;
        #[cfg(windows)]
        let qpc_per_second = {
            let mut frequency = 0;
            unsafe {
                windows_sys::Win32::System::Performance::QueryPerformanceFrequency(
                    &raw mut frequency,
                )
            };
            frequency
        };
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
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            opened_at: Instant::now(),
            #[cfg(windows)]
            wsa_recv_msg,
            #[cfg(windows)]
            qpc_per_second,
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
    ) -> io::Result<()> {
        self.send(self.device_addrs[device], kind, msg_id, body)
            .await
    }

    fn try_send(
        &self,
        dst: SocketAddrV6,
        kind: Kind,
        msg_id: u16,
        body: [&[u8]; 2],
    ) -> io::Result<()> {
        let header = Header::new(kind, msg_id);
        let mut datagram = [0; size_of::<Header>() + FRAME_BYTES_MAX];
        let mut len = 0;
        for part in [header.as_bytes(), body[0], body[1]] {
            datagram[len..len + part.len()].copy_from_slice(part);
            len += part.len();
        }
        loop {
            match self.socket.try_send_to(&datagram[..len], dst.into()) {
                Ok(_) => return Ok(()),
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
    ) -> io::Result<()> {
        loop {
            match self.try_send(dst, kind, msg_id, body) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => self.socket.writable().await?,
                sent => return sent,
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn try_recv_with_arrival(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr, Instant)> {
        use std::os::fd::AsRawFd;

        use nix::sys::socket::{MsgFlags, SockaddrIn6, recvmsg};
        use nix::sys::time::TimeValLike;
        use nix::time::{ClockId, clock_gettime};

        let mut cmsg = nix::cmsg_space!(nix::sys::time::TimeSpec);
        self.socket.try_io(tokio::io::Interest::READABLE, || {
            let mut iov = [io::IoSliceMut::new(buf)];
            let msg = recvmsg::<SockaddrIn6>(
                self.socket.as_raw_fd(),
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
                arrival_instant(now, self.opened_at, queued_ns),
            ))
        })
    }

    #[cfg(windows)]
    fn try_recv_with_arrival(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr, Instant)> {
        use std::os::windows::io::AsRawSocket;

        use windows_sys::Win32::Networking::WinSock::{SOCKET, WSABUF, WSAGetLastError, WSAMSG};
        use windows_sys::Win32::System::Performance::QueryPerformanceCounter;

        let Some(recv_msg) = self.wsa_recv_msg else {
            let (len, src) = self.socket.try_recv_from(buf)?;
            return Ok((len, src, Instant::now()));
        };
        let raw = SOCKET::try_from(self.socket.as_raw_socket())
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        self.socket.try_io(tokio::io::Interest::READABLE, || {
            let mut control = [0u64; 4];
            let mut control_len = 0;
            let mut received = 0;
            let ((), src) = unsafe {
                socket2::SockAddr::try_init(|storage, len| {
                    let mut data = WSABUF {
                        len: u32::try_from(buf.len()).unwrap_or(u32::MAX),
                        buf: buf.as_mut_ptr(),
                    };
                    let mut msg = WSAMSG {
                        name: storage.cast(),
                        namelen: *len,
                        lpBuffers: &raw mut data,
                        dwBufferCount: 1,
                        Control: WSABUF {
                            len: u32::try_from(size_of_val(&control)).unwrap_or(u32::MAX),
                            buf: control.as_mut_ptr().cast(),
                        },
                        dwFlags: 0,
                    };
                    if recv_msg(
                        raw,
                        &raw mut msg,
                        &raw mut received,
                        std::ptr::null_mut(),
                        None,
                    ) != 0
                    {
                        return Err(io::Error::from_raw_os_error(WSAGetLastError()));
                    }
                    *len = msg.namelen;
                    control_len = msg.Control.len;
                    Ok(())
                })?
            };
            let now = Instant::now();
            let mut qpc_now = 0;
            unsafe { QueryPerformanceCounter(&raw mut qpc_now) };
            let queued_ns = control
                .as_bytes()
                .get(..usize::try_from(control_len).unwrap_or(usize::MAX))
                .and_then(wsa_rx_timestamp)
                .and_then(|stamp| queued_ns_from_qpc(qpc_now, stamp, self.qpc_per_second));
            let src = src.as_socket().ok_or(io::ErrorKind::AddrNotAvailable)?;
            Ok((
                usize::try_from(received).unwrap_or(usize::MAX),
                src,
                arrival_instant(now, self.opened_at, queued_ns),
            ))
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    fn try_recv_with_arrival(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr, Instant)> {
        let (len, src) = self.socket.try_recv_from(buf)?;
        Ok((len, src, Instant::now()))
    }

    pub(crate) async fn recv_from_device(&self) -> io::Result<(DeviceReply, Instant)> {
        loop {
            if let Some(reply) = self.try_recv_from_device()? {
                return Ok(reply);
            }
            self.socket.readable().await?;
        }
    }

    #[cfg(test)]
    pub(crate) async fn readable(&self) -> io::Result<()> {
        self.socket.readable().await
    }

    pub(crate) fn try_recv_from_device(&self) -> io::Result<Option<(DeviceReply, Instant)>> {
        let mut buf = [0; RECV_BUFFER_BYTES];
        loop {
            let (len, src, received_at) =
                match accepted_datagram(self.try_recv_with_arrival(&mut buf)) {
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
                return Ok(Some((parsed, received_at)));
            }
        }
    }

    pub(crate) async fn recv_datagram<'a>(
        &self,
        buf: &'a mut [u8; RECV_BUFFER_BYTES],
        deadline: Option<Instant>,
    ) -> io::Result<Option<(SocketAddrV6, &'a [u8])>> {
        let (len, src) = loop {
            match accepted_datagram(
                self.socket
                    .try_recv_from(&mut buf[..])
                    .map(|(len, src)| (len, src, ())),
            ) {
                Ok(Some((len, src, ()))) => break (len, src),
                Ok(None) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => match deadline {
                    Some(deadline) => {
                        match tokio::time::timeout_at(deadline.into(), self.socket.readable()).await
                        {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn read_at_and_opened_a_second_before() -> (Instant, Instant) {
        let now = Instant::now();
        (
            now,
            now.checked_sub(std::time::Duration::from_secs(1)).unwrap(),
        )
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn a_reply_queued_in_the_kernel_is_stamped_at_its_arrival() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(
            arrival_instant(now, opened_at, Some(5_000)),
            now.checked_sub(std::time::Duration::from_micros(5))
                .unwrap()
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn a_reply_without_a_kernel_timestamp_is_stamped_when_read() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, None), now);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn a_kernel_timestamp_later_than_the_read_is_ignored() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, Some(-1)), now);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn a_kernel_timestamp_earlier_than_the_open_is_ignored() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, Some(2_000_000_000)), now);
    }

    fn wsa_cmsg(level: i32, kind: i32, data: &[u8]) -> Vec<u8> {
        let word = size_of::<usize>();
        let data_offset = (word + 8).next_multiple_of(word);
        let mut cmsg = Vec::new();
        cmsg.extend_from_slice(&(data_offset + data.len()).to_ne_bytes());
        cmsg.extend_from_slice(&level.to_ne_bytes());
        cmsg.extend_from_slice(&kind.to_ne_bytes());
        cmsg.resize(data_offset, 0);
        cmsg.extend_from_slice(data);
        cmsg.resize(cmsg.len().next_multiple_of(word), 0);
        cmsg
    }

    #[test]
    fn the_wsa_receive_timestamp_is_read_from_its_control_message() {
        let control = wsa_cmsg(0xFFFF, 0x300A, &1_234_u64.to_ne_bytes());
        assert_eq!(wsa_rx_timestamp(&control), Some(1_234));
    }

    #[test]
    fn other_wsa_control_messages_before_the_timestamp_are_skipped() {
        let mut control = wsa_cmsg(41, 50, &[7; 20]);
        control.extend(wsa_cmsg(0xFFFF, 0x300A, &5_678_u64.to_ne_bytes()));
        assert_eq!(wsa_rx_timestamp(&control), Some(5_678));
    }

    #[test]
    fn a_truncated_wsa_timestamp_is_absent() {
        let control = wsa_cmsg(0xFFFF, 0x300A, &1_234_u64.to_ne_bytes());
        assert_eq!(wsa_rx_timestamp(&control[..control.len() - 1]), None);
        assert_eq!(wsa_rx_timestamp(&[]), None);
    }

    #[test]
    fn qpc_ticks_queued_are_converted_to_nanoseconds() {
        assert_eq!(
            queued_ns_from_qpc(10_000_050, 10_000_000, 10_000_000),
            Some(5_000)
        );
        assert_eq!(
            queued_ns_from_qpc(10_000_000, 10_000_050, 10_000_000),
            Some(-5_000)
        );
        assert_eq!(queued_ns_from_qpc(10_000_050, 10_000_000, 0), None);
    }

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
            .send(dst, Kind::Frame, 0x1234, [&first, &second])
            .await
            .unwrap();

        let mut buf = [0; RECV_BUFFER_BYTES];
        let (len, _) = receiver.recv_from(&mut buf).unwrap();
        let header = Header::new(Kind::Frame, 0x1234);
        let expected = [header.as_bytes(), &first, &second].concat();
        assert_eq!(&buf[..len], expected);
    }
}

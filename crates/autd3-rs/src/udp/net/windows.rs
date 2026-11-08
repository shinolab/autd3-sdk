use std::io;
use std::net::SocketAddr;
use std::time::Instant;

use socket2::Socket;
use tokio::net::UdpSocket;

use super::arrival_instant;
use super::cmsg::{queued_ns_from_qpc, wsa_rx_timestamp};

pub(in crate::udp) struct Stamping {
    wsa_recv_msg: windows_sys::Win32::Networking::WinSock::LPFN_WSARECVMSG,
    qpc_per_second: i64,
}

impl Stamping {
    pub(in crate::udp) fn open(socket: &Socket) -> io::Result<Self> {
        let wsa_recv_msg = enable_wsa_rx_timestamps(socket)?;
        let qpc_per_second = {
            let mut frequency = 0;
            unsafe {
                windows_sys::Win32::System::Performance::QueryPerformanceFrequency(
                    &raw mut frequency,
                )
            };
            frequency
        };
        Ok(Self {
            wsa_recv_msg,
            qpc_per_second,
        })
    }

    pub(in crate::udp) fn try_recv(
        &self,
        socket: &UdpSocket,
        buf: &mut [u8],
        opened_at: Instant,
    ) -> io::Result<(usize, SocketAddr, Instant)> {
        use std::os::windows::io::AsRawSocket;

        use windows_sys::Win32::Networking::WinSock::{SOCKET, WSABUF, WSAGetLastError, WSAMSG};
        use windows_sys::Win32::System::Performance::QueryPerformanceCounter;
        use zerocopy::IntoBytes;

        let Some(recv_msg) = self.wsa_recv_msg else {
            let (len, src) = socket.try_recv_from(buf)?;
            return Ok((len, src, Instant::now()));
        };
        let raw =
            SOCKET::try_from(socket.as_raw_socket()).map_err(|_| io::ErrorKind::InvalidInput)?;
        socket.try_io(tokio::io::Interest::READABLE, || {
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
            let mut qpc_now = 0;
            unsafe { QueryPerformanceCounter(&raw mut qpc_now) };
            let now = Instant::now();
            let queued_ns = control
                .as_bytes()
                .get(..usize::try_from(control_len).unwrap_or(usize::MAX))
                .and_then(wsa_rx_timestamp)
                .and_then(|stamp| queued_ns_from_qpc(qpc_now, stamp, self.qpc_per_second));
            let src = src.as_socket().ok_or(io::ErrorKind::AddrNotAvailable)?;
            Ok((
                usize::try_from(received).unwrap_or(usize::MAX),
                src,
                arrival_instant(now, opened_at, queued_ns),
            ))
        })
    }
}

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

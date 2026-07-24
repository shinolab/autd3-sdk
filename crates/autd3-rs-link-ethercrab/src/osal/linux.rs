use std::future::Future;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use ethercrab::error::Error as EtherCrabError;
use ethercrab::{PduRx, PduTx};
use tokio::io::unix::AsyncFd;

const ETHERCAT_ETHERTYPE: u16 = 0x88a4;
const ETHERNET_OVERHEAD: usize = 18;
const RX_BUDGET: u32 = 32;

#[repr(C)]
#[allow(non_camel_case_types)]
struct ifreq {
    ifr_name: [libc::c_char; libc::IF_NAMESIZE],
    ifr_data: libc::c_int,
}

fn ifreq_for(name: &str) -> io::Result<ifreq> {
    let bytes = name.as_bytes();
    if bytes.len() >= libc::IF_NAMESIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("interface name {name:?} is too long"),
        ));
    }
    let mut ifreq = ifreq {
        ifr_name: [0; libc::IF_NAMESIZE],
        ifr_data: 0,
    };
    for (dst, byte) in ifreq.ifr_name.iter_mut().zip(bytes) {
        *dst = libc::c_char::from_ne_bytes([*byte]);
    }
    Ok(ifreq)
}

fn ifreq_ioctl(fd: RawFd, ifreq: &mut ifreq, cmd: libc::c_ulong) -> io::Result<libc::c_int> {
    // SAFETY: `fd` is a valid socket and `ifreq` is a live, correctly shaped
    // `struct ifreq` for the `SIOCGIF*` commands used here, which only read
    // `ifr_name` and write `ifr_data`.
    let res = unsafe {
        #[cfg(target_env = "musl")]
        let cmd = libc::c_int::try_from(cmd).map_err(|_| io::ErrorKind::InvalidInput)?;
        libc::ioctl(fd, cmd, std::ptr::from_mut(ifreq))
    };
    if res == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(ifreq.ifr_data)
}

pub(crate) struct RawSocket {
    fd: OwnedFd,
}

impl RawSocket {
    fn new(interface: &str) -> io::Result<Self> {
        let mut ifreq = ifreq_for(interface)?;

        // SAFETY: a plain `socket(2)` call; the returned descriptor is checked
        // below and immediately taken ownership of by `OwnedFd`.
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                i32::from(ETHERCAT_ETHERTYPE.to_be()),
            )
        };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just returned by `socket(2)` and is not owned elsewhere.
        let this = Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        };

        let sockaddr = libc::sockaddr_ll {
            sll_family: u16::try_from(libc::AF_PACKET).expect("AF_PACKET fits in u16"),
            sll_protocol: ETHERCAT_ETHERTYPE.to_be(),
            sll_ifindex: ifreq_ioctl(this.as_raw_fd(), &mut ifreq, libc::SIOCGIFINDEX)?,
            sll_hatype: 1,
            sll_pkttype: 0,
            sll_halen: 6,
            sll_addr: [0; 8],
        };
        // SAFETY: `sockaddr` is a live `sockaddr_ll` and the length passed matches it.
        let res = unsafe {
            libc::bind(
                this.as_raw_fd(),
                std::ptr::from_ref(&sockaddr).cast(),
                libc::socklen_t::try_from(size_of::<libc::sockaddr_ll>())
                    .expect("sockaddr_ll fits in socklen_t"),
            )
        };
        if res == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(this)
    }

    fn interface_mtu(&self, interface: &str) -> io::Result<usize> {
        let mut ifreq = ifreq_for(interface)?;
        let mtu = ifreq_ioctl(self.as_raw_fd(), &mut ifreq, libc::SIOCGIFMTU)?;
        usize::try_from(mtu).map_err(|_| io::Error::other("interface reported a negative MTU"))
    }

    fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: `buf` is a live, writable slice of `buf.len()` bytes.
        let len = unsafe { libc::read(self.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if len == -1 {
            return Err(io::Error::last_os_error());
        }
        usize::try_from(len).map_err(|_| io::Error::other("read(2) returned a negative length"))
    }

    fn send(&self, buf: &[u8]) -> io::Result<usize> {
        // SAFETY: `buf` is a live, readable slice of `buf.len()` bytes.
        let len = unsafe { libc::write(self.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
        if len == -1 {
            return Err(io::Error::last_os_error());
        }
        usize::try_from(len).map_err(|_| io::Error::other("write(2) returned a negative length"))
    }
}

impl AsRawFd for RawSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsFd for RawSocket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

struct TxRxFut<'sto> {
    socket: AsyncFd<RawSocket>,
    buf: Box<[u8]>,
    tx: Option<PduTx<'sto>>,
    rx: Option<PduRx<'sto>>,
}

impl TxRxFut<'_> {
    fn release(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _released = tx.release();
        }
        if let Some(rx) = self.rx.take() {
            let _released = rx.release();
        }
    }
}

impl Future for TxRxFut<'_> {
    type Output = Result<(), EtherCrabError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let (Some(tx), Some(rx)) = (this.tx.as_mut(), this.rx.as_mut()) else {
            return Poll::Ready(Ok(()));
        };

        tx.replace_waker(cx.waker());
        if tx.should_exit() {
            tracing::debug!("tx/rx future was asked to exit");
            this.release();
            return Poll::Ready(Ok(()));
        }

        loop {
            let mut guard = match this.socket.poll_write_ready(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(e)) => {
                    tracing::error!("waiting for interface writability failed: {e}");
                    return Poll::Ready(Err(EtherCrabError::SendFrame));
                }
                Poll::Pending => break,
            };
            let Some(frame) = tx.next_sendable_frame() else {
                break;
            };

            let mut blocked = false;
            let sent = frame.send_blocking(|data| {
                match guard.try_io(|socket| socket.get_ref().send(data)) {
                    Ok(Ok(n)) => Ok(n),
                    Ok(Err(e)) => {
                        tracing::error!("sending PDU failed: {e}");
                        Err(EtherCrabError::SendFrame)
                    }
                    Err(_would_block) => {
                        blocked = true;
                        Ok(0)
                    }
                }
            });
            if blocked {
                continue;
            }
            if let Err(e) = sent {
                return Poll::Ready(Err(e));
            }
        }

        let mut budget = RX_BUDGET;
        loop {
            let mut guard = match this.socket.poll_read_ready(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(e)) => {
                    tracing::error!("waiting for interface readability failed: {e}");
                    return Poll::Ready(Err(EtherCrabError::ReceiveFrame));
                }
                Poll::Pending => break,
            };
            match guard.try_io(|socket| socket.get_ref().recv(&mut this.buf)) {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => {
                    if let Err(e) = rx.receive_frame(&this.buf[..n]) {
                        tracing::trace!("skipping unprocessable RX frame: {e}");
                    }
                    budget -= 1;
                    if budget == 0 {
                        cx.waker().wake_by_ref();
                        break;
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("receiving PDU failed: {e}");
                    return Poll::Ready(Err(EtherCrabError::ReceiveFrame));
                }
                Err(_would_block) => {}
            }
        }

        Poll::Pending
    }
}

pub(crate) fn tx_rx_task<'sto>(
    interface: &str,
    pdu_tx: PduTx<'sto>,
    pdu_rx: PduRx<'sto>,
) -> Result<impl Future<Output = Result<(), EtherCrabError>> + 'sto, io::Error> {
    let socket = RawSocket::new(interface)?;
    let mtu = socket.interface_mtu(interface)?;
    tracing::debug!("opening {interface} with MTU {mtu}");

    Ok(TxRxFut {
        socket: AsyncFd::new(socket)?,
        buf: vec![0u8; mtu + ETHERNET_OVERHEAD].into_boxed_slice(),
        tx: Some(pdu_tx),
        rx: Some(pdu_rx),
    })
}

#[cfg(test)]
mod tests {
    use super::{ETHERCAT_ETHERTYPE, RawSocket, ifreq_for};

    #[test]
    #[ignore = "needs CAP_NET_RAW: run under `unshare -rn`"]
    fn raw_socket_loops_an_ethercat_frame_back_on_lo() {
        let socket = RawSocket::new("lo").expect("open lo");
        assert!(socket.interface_mtu("lo").expect("mtu") >= 1500);

        let mut frame = [0u8; 64];
        frame[0..6].copy_from_slice(&[0xff; 6]);
        frame[6..12].copy_from_slice(&[0x10; 6]);
        frame[12..14].copy_from_slice(&ETHERCAT_ETHERTYPE.to_be_bytes());
        frame[14] = 0xa5;
        assert_eq!(socket.send(&frame).expect("send"), frame.len());

        let mut buf = [0u8; 128];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let n = loop {
            match socket.recv(&mut buf) {
                Ok(n) => break n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "no frame looped back");
                }
                Err(e) => panic!("recv failed: {e}"),
            }
        };
        assert_eq!(&buf[..n], &frame[..]);
    }

    #[test]
    fn ifreq_for_copies_a_nul_terminated_name() {
        let ifreq = ifreq_for("eth0").expect("short name");
        let name: Vec<u8> = ifreq
            .ifr_name
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| c.to_ne_bytes()[0])
            .collect();
        assert_eq!(name, b"eth0");
        assert_eq!(ifreq.ifr_name[libc::IF_NAMESIZE - 1], 0);
    }

    #[test]
    fn ifreq_for_rejects_a_name_that_leaves_no_room_for_the_nul() {
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE - 1)).is_ok());
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE)).is_err());
    }

    #[test]
    fn ethertype_is_sent_in_network_byte_order() {
        assert_eq!(ETHERCAT_ETHERTYPE.to_be_bytes(), [0x88, 0xa4]);
        assert_eq!(ETHERCAT_ETHERTYPE.to_be().to_ne_bytes(), [0x88, 0xa4]);
    }
}

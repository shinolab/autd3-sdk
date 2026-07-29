use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

use super::RawBus;
use crate::wire::ETHERTYPE_ETHERCAT;

const PACKET_OUTGOING: u8 = 4;

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
    // SAFETY: `fd` is a valid socket and `ifreq` is a live, correctly shaped `struct ifreq`
    // for the `SIOCGIF*` commands used here, which only read `ifr_name` and write `ifr_data`.
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

pub fn interface_candidates() -> io::Result<Vec<String>> {
    let mut names = std::fs::read_dir("/sys/class/net")?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name != "lo")
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

pub struct RawSocket {
    fd: OwnedFd,
    mtu: usize,
}

impl RawSocket {
    pub fn open(interface: &str) -> io::Result<Self> {
        let mut ifreq = ifreq_for(interface)?;

        // SAFETY: a plain `socket(2)` call; the returned descriptor is checked below and
        // immediately taken ownership of by `OwnedFd`.
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                i32::from(ETHERTYPE_ETHERCAT.to_be()),
            )
        };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just returned by `socket(2)` and is not owned elsewhere.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let index = ifreq_ioctl(fd.as_raw_fd(), &mut ifreq, libc::SIOCGIFINDEX)?;
        let sockaddr = libc::sockaddr_ll {
            sll_family: u16::try_from(libc::AF_PACKET).expect("AF_PACKET fits in u16"),
            sll_protocol: ETHERTYPE_ETHERCAT.to_be(),
            sll_ifindex: index,
            sll_hatype: 1,
            sll_pkttype: 0,
            sll_halen: 6,
            sll_addr: [0; 8],
        };
        // SAFETY: `sockaddr` is a live `sockaddr_ll` and the length passed matches it.
        let res = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                std::ptr::from_ref(&sockaddr).cast(),
                libc::socklen_t::try_from(size_of::<libc::sockaddr_ll>())
                    .expect("sockaddr_ll fits in socklen_t"),
            )
        };
        if res == -1 {
            return Err(io::Error::last_os_error());
        }

        let mtu = ifreq_ioctl(fd.as_raw_fd(), &mut ifreq, libc::SIOCGIFMTU)?;
        let mtu = usize::try_from(mtu)
            .map_err(|_| io::Error::other("interface reported a negative MTU"))?;

        Ok(Self { fd, mtu })
    }

    fn recv_once(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let mut src = std::mem::MaybeUninit::<libc::sockaddr_ll>::zeroed();
        let mut src_len =
            libc::socklen_t::try_from(size_of::<libc::sockaddr_ll>()).expect("fits in socklen_t");
        // SAFETY: `buf` is a live writable slice, and `src`/`src_len` are a matching
        // `sockaddr_ll` out-parameter pair that `recvfrom(2)` fills in.
        let len = unsafe {
            libc::recvfrom(
                self.fd.as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                src.as_mut_ptr().cast(),
                &raw mut src_len,
            )
        };
        if len == -1 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(err);
        }
        // SAFETY: `recvfrom(2)` succeeded, so it initialised `src` with the peer address.
        let pkttype = unsafe { src.assume_init() }.sll_pkttype;
        if pkttype == PACKET_OUTGOING {
            return Ok(None);
        }
        let len = usize::try_from(len)
            .map_err(|_| io::Error::other("recvfrom returned a negative length"))?;
        Ok(Some(len))
    }

    fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        let mut pollfd = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = libc::c_int::try_from(timeout.as_nanos().div_ceil(1_000_000))
            .unwrap_or(libc::c_int::MAX);
        // SAFETY: `pollfd` is a live, correctly initialised single-element array.
        let res = unsafe { libc::poll(&raw mut pollfd, 1, millis) };
        if res == -1 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(err);
        }
        Ok(res > 0)
    }
}

impl RawBus for RawSocket {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        // SAFETY: `frame` is a live, readable slice of `frame.len()` bytes.
        let len = unsafe { libc::write(self.fd.as_raw_fd(), frame.as_ptr().cast(), frame.len()) };
        if len == -1 {
            return Err(io::Error::last_os_error());
        }
        let len = usize::try_from(len)
            .map_err(|_| io::Error::other("write returned a negative length"))?;
        if len != frame.len() {
            return Err(io::Error::other("interface accepted a partial frame"));
        }
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(len) = self.recv_once(buf)? {
                return Ok(Some(len));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            if !self.wait_readable(deadline - now)? {
                let now = Instant::now();
                if now >= deadline {
                    return Ok(None);
                }
            }
        }
    }

    fn mtu(&self) -> usize {
        self.mtu
    }
}

#[cfg(test)]
mod tests {
    use super::{RawSocket, ifreq_for, interface_candidates};
    use crate::bus::RawBus;
    use crate::wire::ETHERTYPE_ETHERCAT;
    use std::time::Duration;

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
    fn interface_candidates_never_offers_loopback() {
        let candidates = interface_candidates().expect("sysfs is readable");
        assert!(!candidates.iter().any(|name| name == "lo"));
    }

    #[test]
    #[ignore = "needs CAP_NET_RAW: run under `unshare -rn`"]
    fn own_frames_are_filtered_out_of_the_receive_path() {
        let mut socket = RawSocket::open("lo").expect("open lo");
        assert!(socket.mtu() >= 1500);

        let mut frame = [0u8; 60];
        frame[0..6].copy_from_slice(&[0xff; 6]);
        frame[6..12].copy_from_slice(&[0x10; 6]);
        frame[12..14].copy_from_slice(&ETHERTYPE_ETHERCAT.to_be_bytes());
        socket.send(&frame).expect("send");

        let mut buf = [0u8; 128];
        let received = socket
            .receive(&mut buf, Duration::from_millis(200))
            .expect("receive");
        assert_eq!(
            received,
            Some(frame.len()),
            "loopback delivers the frame back as PACKET_HOST"
        );
    }
}

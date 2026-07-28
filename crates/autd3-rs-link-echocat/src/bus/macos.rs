use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

use super::RawBus;
use crate::wire::ETHERTYPE_ETHERCAT;

const BPF_DEVICES: u32 = 256;
const BPF_BUFFER_BYTES: libc::c_uint = 1 << 16;
const MTU: usize = 1500;
const DLT_EN10MB: libc::c_uint = 1;

const BPF_ALIGNMENT: usize = 4;
const _: () = assert!(libc::BPF_ALIGNMENT == 4);

const BH_CAPLEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_caplen);
const BH_DATALEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_datalen);
const BH_HDRLEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_hdrlen);
const BPF_HDR_BYTES: usize = BH_HDRLEN + size_of::<libc::c_ushort>();
const _: () = assert!(BPF_HDR_BYTES == 18 && BPF_HDR_BYTES < size_of::<libc::bpf_hdr>());

#[repr(C)]
#[allow(non_camel_case_types)]
struct ifreq {
    ifr_name: [libc::c_char; libc::IF_NAMESIZE],
    ifr_ifru: [u8; 16],
}

const _: () = assert!(size_of::<ifreq>() == size_of::<libc::ifreq>());

#[repr(C)]
struct BpfInsn {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

#[repr(C)]
struct BpfProgram {
    bf_len: libc::c_uint,
    bf_insns: *const BpfInsn,
}

const BPF_LDH_ABS: u16 = 0x28;
const BPF_JEQ_K: u16 = 0x15;
const BPF_RET_K: u16 = 0x06;

const ETHERCAT_ETHERTYPE: u32 = u32::from_be_bytes([
    0,
    0,
    ETHERTYPE_ETHERCAT.to_be_bytes()[0],
    ETHERTYPE_ETHERCAT.to_be_bytes()[1],
]);

static ETHERCAT_FILTER: [BpfInsn; 4] = [
    BpfInsn {
        code: BPF_LDH_ABS,
        jt: 0,
        jf: 0,
        k: 12,
    },
    BpfInsn {
        code: BPF_JEQ_K,
        jt: 0,
        jf: 1,
        k: ETHERCAT_ETHERTYPE,
    },
    BpfInsn {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: u32::MAX,
    },
    BpfInsn {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: 0,
    },
];

const fn bpf_wordalign(len: usize) -> usize {
    (len + BPF_ALIGNMENT - 1) & !(BPF_ALIGNMENT - 1)
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
        ifr_ifru: [0; 16],
    };
    for (dst, byte) in ifreq.ifr_name.iter_mut().zip(bytes) {
        *dst = libc::c_char::from_ne_bytes([*byte]);
    }
    Ok(ifreq)
}

fn ioctl(fd: RawFd, cmd: libc::c_ulong, arg: *mut libc::c_void) -> io::Result<()> {
    // SAFETY: `fd` is a live descriptor and `arg` points to a live value of the
    // type `cmd` encodes, which every caller below upholds.
    if unsafe { libc::ioctl(fd, cmd, arg) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes(bytes[at..at + 2].try_into().expect("2 bytes"))
}

fn open_device() -> io::Result<OwnedFd> {
    let mut first_error = None;
    for i in 0..BPF_DEVICES {
        let path = format!("/dev/bpf{i}\0");
        // SAFETY: `path` is NUL-terminated and outlives the call; the returned
        // descriptor is checked before being taken ownership of.
        let fd = unsafe {
            libc::open(
                path.as_ptr().cast(),
                libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd != -1 {
            // SAFETY: `fd` was just returned by `open(2)` and is not owned elsewhere.
            return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
        }
        first_error.get_or_insert_with(io::Error::last_os_error);
    }
    Err(first_error.unwrap_or_else(|| io::Error::other("no BPF device to open")))
}

pub fn interface_candidates() -> io::Result<Vec<String>> {
    // SAFETY: `if_nameindex` allocates a NUL-terminated array that is released
    // by the matching `if_freenameindex` below.
    let head = unsafe { libc::if_nameindex() };
    if head.is_null() {
        return Err(io::Error::last_os_error());
    }

    let mut names = Vec::new();
    let mut entry = head;
    loop {
        // SAFETY: `entry` walks the array returned by `if_nameindex`, whose last
        // element is zeroed, so the loop stops before running past the end.
        let current = unsafe { &*entry };
        if current.if_index == 0 || current.if_name.is_null() {
            break;
        }
        // SAFETY: `if_name` is a NUL-terminated string owned by the array.
        let name = unsafe { std::ffi::CStr::from_ptr(current.if_name) };
        if let Ok(name) = name.to_str()
            && !name.starts_with("lo")
        {
            names.push(name.to_owned());
        }
        // SAFETY: still inside the array; the zeroed terminator ends the walk.
        entry = unsafe { entry.add(1) };
    }

    // SAFETY: `head` came from `if_nameindex` and has not been released yet.
    unsafe { libc::if_freenameindex(head) };

    names.sort();
    names.dedup();
    Ok(names)
}

struct Records<'a> {
    buf: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for Records<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        loop {
            let header = self
                .buf
                .get(self.offset..self.offset.checked_add(BPF_HDR_BYTES)?)?;
            let caplen = read_u32(header, BH_CAPLEN) as usize;
            let datalen = read_u32(header, BH_DATALEN) as usize;
            let hdrlen = read_u16(header, BH_HDRLEN) as usize;
            if hdrlen < BPF_HDR_BYTES {
                tracing::warn!("BPF header length {hdrlen} is impossibly short");
                return None;
            }

            let start = self.offset + hdrlen;
            let frame = self.buf.get(start..start.checked_add(caplen)?)?;
            self.offset = bpf_wordalign(start + caplen);

            if caplen != datalen {
                tracing::warn!("skipping a frame truncated to {caplen} of {datalen} B");
                continue;
            }
            return Some(frame);
        }
    }
}

pub struct RawSocket {
    fd: OwnedFd,
    buffer: Box<[u8]>,
    filled: usize,
    offset: usize,
}

impl RawSocket {
    pub fn open(interface: &str) -> io::Result<Self> {
        let fd = open_device()?;
        let raw = fd.as_raw_fd();

        let mut buffer_len: libc::c_uint = BPF_BUFFER_BYTES;
        ioctl(
            raw,
            libc::BIOCSBLEN,
            std::ptr::from_mut(&mut buffer_len).cast(),
        )?;

        let mut ifreq = ifreq_for(interface)?;
        ioctl(raw, libc::BIOCSETIF, std::ptr::from_mut(&mut ifreq).cast())?;

        let mut datalink: libc::c_uint = 0;
        ioctl(
            raw,
            libc::BIOCGDLT,
            std::ptr::from_mut(&mut datalink).cast(),
        )?;
        if datalink != DLT_EN10MB {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{interface} is not an ethernet interface"),
            ));
        }

        let mut enable: libc::c_uint = 1;
        ioctl(
            raw,
            libc::BIOCIMMEDIATE,
            std::ptr::from_mut(&mut enable).cast(),
        )?;
        ioctl(
            raw,
            libc::BIOCSHDRCMPLT,
            std::ptr::from_mut(&mut enable).cast(),
        )?;

        let mut disable: libc::c_uint = 0;
        ioctl(
            raw,
            libc::BIOCSSEESENT,
            std::ptr::from_mut(&mut disable).cast(),
        )?;

        let program = BpfProgram {
            bf_len: u32::try_from(ETHERCAT_FILTER.len()).expect("4 instructions fit in u32"),
            bf_insns: ETHERCAT_FILTER.as_ptr(),
        };
        ioctl(
            raw,
            libc::BIOCSETF,
            std::ptr::from_ref(&program).cast_mut().cast(),
        )?;

        let mut accepted: libc::c_uint = 0;
        ioctl(
            raw,
            libc::BIOCGBLEN,
            std::ptr::from_mut(&mut accepted).cast(),
        )?;

        Ok(Self {
            fd,
            buffer: vec![0u8; accepted as usize].into_boxed_slice(),
            filled: 0,
            offset: 0,
        })
    }

    fn take_record(&mut self, buf: &mut [u8]) -> Option<usize> {
        let mut records = Records {
            buf: &self.buffer[..self.filled],
            offset: self.offset,
        };
        let frame = records.next()?;
        let len = frame.len().min(buf.len());
        buf[..len].copy_from_slice(&frame[..len]);
        self.offset = records.offset;
        Some(len)
    }

    fn fill(&mut self) -> io::Result<bool> {
        // SAFETY: `self.buffer` is a live, writable slice of `self.buffer.len()` bytes.
        let len = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                self.buffer.as_mut_ptr().cast(),
                self.buffer.len(),
            )
        };
        if len == -1 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock || err.kind() == io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(err);
        }
        self.filled = usize::try_from(len)
            .map_err(|_| io::Error::other("read returned a negative length"))?;
        self.offset = 0;
        Ok(true)
    }

    fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        let mut pollfd = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
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
            if let Some(len) = self.take_record(buf) {
                return Ok(Some(len));
            }
            if self.fill()? {
                continue;
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            self.wait_readable(deadline - now)?;
        }
    }

    fn mtu(&self) -> usize {
        MTU
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BH_CAPLEN, BH_DATALEN, BH_HDRLEN, BPF_HDR_BYTES, ETHERCAT_FILTER, ETHERTYPE_ETHERCAT,
        Records, bpf_wordalign, ifreq_for, interface_candidates,
    };

    fn push_record(buf: &mut Vec<u8>, frame: &[u8], datalen: usize) {
        let start = buf.len();
        buf.resize(start + BPF_HDR_BYTES, 0);
        let header = &mut buf[start..];
        header[BH_CAPLEN..BH_CAPLEN + 4]
            .copy_from_slice(&u32::try_from(frame.len()).unwrap().to_ne_bytes());
        header[BH_DATALEN..BH_DATALEN + 4]
            .copy_from_slice(&u32::try_from(datalen).unwrap().to_ne_bytes());
        header[BH_HDRLEN..BH_HDRLEN + 2]
            .copy_from_slice(&u16::try_from(BPF_HDR_BYTES).unwrap().to_ne_bytes());
        buf.extend_from_slice(frame);
        buf.resize(bpf_wordalign(buf.len()), 0);
    }

    #[test]
    fn a_single_read_yields_every_frame_it_carries() {
        let mut buf = Vec::new();
        push_record(&mut buf, &[0xaa; 60], 60);
        push_record(&mut buf, &[0xbb; 61], 61);
        push_record(&mut buf, &[0xcc; 60], 60);

        let frames: Vec<&[u8]> = Records {
            buf: &buf,
            offset: 0,
        }
        .collect();
        assert_eq!(frames, [&[0xaa; 60][..], &[0xbb; 61][..], &[0xcc; 60][..]]);
    }

    #[test]
    fn an_unaligned_frame_still_lands_the_next_record_on_a_word_boundary() {
        let mut buf = Vec::new();
        push_record(&mut buf, &[0xbb; 61], 61);
        let tail = buf.len();
        push_record(&mut buf, &[0xcc; 60], 60);

        assert_eq!(tail % 4, 0);
        assert_eq!(bpf_wordalign(BPF_HDR_BYTES + 61), tail);

        let frames: Vec<&[u8]> = Records {
            buf: &buf,
            offset: 0,
        }
        .collect();
        assert_eq!(frames, [&[0xbb; 61][..], &[0xcc; 60][..]]);
    }

    #[test]
    fn a_truncated_record_is_dropped_but_its_successor_is_not() {
        let mut buf = Vec::new();
        push_record(&mut buf, &[0xaa; 60], 1514);
        push_record(&mut buf, &[0xcc; 60], 60);

        let frames: Vec<&[u8]> = Records {
            buf: &buf,
            offset: 0,
        }
        .collect();
        assert_eq!(frames, [&[0xcc; 60][..]]);
    }

    #[test]
    fn a_record_running_past_the_read_length_is_dropped() {
        let mut buf = Vec::new();
        push_record(&mut buf, &[0xaa; 60], 60);
        let complete = buf.len();
        push_record(&mut buf, &[0xbb; 60], 60);

        let frames: Vec<&[u8]> = Records {
            buf: &buf[..complete + BPF_HDR_BYTES + 10],
            offset: 0,
        }
        .collect();
        assert_eq!(frames, [&[0xaa; 60][..]]);
    }

    #[test]
    fn the_filter_matches_the_ethertype_at_the_ethernet_header_offset() {
        assert_eq!(ETHERCAT_FILTER[0].k, 12);
        assert_eq!(ETHERCAT_FILTER[1].k, u32::from(ETHERTYPE_ETHERCAT));
        assert_eq!(ETHERCAT_FILTER[2].k, u32::MAX);
        assert_eq!(ETHERCAT_FILTER[3].k, 0);
    }

    #[test]
    fn ifreq_for_rejects_a_name_that_leaves_no_room_for_the_nul() {
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE - 1)).is_ok());
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE)).is_err());
    }

    #[test]
    fn interface_candidates_never_offers_loopback() {
        let candidates = interface_candidates().expect("the interface list is readable");
        assert!(!candidates.iter().any(|name| name == "lo0"));
    }
}

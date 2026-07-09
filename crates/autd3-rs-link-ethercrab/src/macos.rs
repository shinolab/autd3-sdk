use std::future::Future;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use ethercrab::error::Error as EtherCrabError;
use ethercrab::{PduRx, PduTx};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

const ETHERCAT_ETHERTYPE: u32 = 0x88a4;
const BPF_DEVICES: u32 = 256;
const BPF_BUFFER_LEN: libc::c_uint = 1 << 16;
// Cap the reads drained per poll so a busy interface cannot starve the executor.
const RX_BUDGET: u32 = 32;

const BPF_ALIGNMENT: usize = 4;
const _: () = assert!(libc::BPF_ALIGNMENT == 4);

const BH_CAPLEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_caplen);
const BH_DATALEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_datalen);
const BH_HDRLEN: usize = std::mem::offset_of!(libc::bpf_hdr, bh_hdrlen);
// `bpf_hdr` carries tail padding, so the record header is shorter than `size_of` reports.
const BPF_HDR_LEN: usize = BH_HDRLEN + size_of::<libc::c_ushort>();
const _: () = assert!(BPF_HDR_LEN == 18 && BPF_HDR_LEN < size_of::<libc::bpf_hdr>());

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

// BPF_LD | BPF_H | BPF_ABS
const BPF_LDH_ABS: u16 = 0x28;
// BPF_JMP | BPF_JEQ | BPF_K
const BPF_JEQ_K: u16 = 0x15;
// BPF_RET | BPF_K
const BPF_RET_K: u16 = 0x06;

// Keep EtherCAT frames, drop everything else before it reaches user space.
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
    // SAFETY: `fd` is a live BPF descriptor and `arg` points to a live value of
    // the type `cmd` encodes, which every caller below upholds.
    if unsafe { libc::ioctl(fd, cmd, arg) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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

pub(crate) struct RawSocket {
    fd: OwnedFd,
    buffer_len: usize,
}

impl RawSocket {
    fn new(interface: &str) -> io::Result<Self> {
        let fd = open_device()?;
        let raw = fd.as_raw_fd();

        // `BIOCSBLEN` is only honoured before the descriptor is attached to an interface.
        let mut buffer_len: libc::c_uint = BPF_BUFFER_LEN;
        ioctl(
            raw,
            libc::BIOCSBLEN,
            std::ptr::from_mut(&mut buffer_len).cast(),
        )?;

        let mut ifreq = ifreq_for(interface)?;
        ioctl(raw, libc::BIOCSETIF, std::ptr::from_mut(&mut ifreq).cast())?;

        let mut enable: libc::c_uint = 1;
        ioctl(
            raw,
            libc::BIOCIMMEDIATE,
            std::ptr::from_mut(&mut enable).cast(),
        )?;
        // Leave the source MAC ethercrab wrote in place, so its own filter recognises
        // the frames we sent and `receive_frame` ignores them.
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
            buffer_len: accepted as usize,
        })
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

// A single `read(2)` returns several `[bpf_hdr, Ethernet frame]` records back to back.
struct BpfRecords<'a> {
    buf: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for BpfRecords<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        loop {
            let header = self
                .buf
                .get(self.offset..self.offset.checked_add(BPF_HDR_LEN)?)?;
            let caplen = read_u32(header, BH_CAPLEN) as usize;
            let datalen = read_u32(header, BH_DATALEN) as usize;
            let hdrlen = read_u16(header, BH_HDRLEN) as usize;
            if hdrlen < BPF_HDR_LEN {
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

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes(bytes[at..at + 2].try_into().expect("2 bytes"))
}

fn is_transient(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(libc::ENOBUFS)
}

struct TxRxFut<'sto> {
    socket: AsyncFd<RawSocket>,
    buf: Box<[u8]>,
    tx: Option<PduTx<'sto>>,
    rx: Option<PduRx<'sto>>,
}

impl TxRxFut<'_> {
    // `release` clears the storage's exit flag so the `PduStorage` can be split again.
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

        // BPF descriptors reject kqueue's write filter, so there is no writability to
        // wait on: send eagerly and retry on the next poll if the kernel pushes back.
        while let Some(frame) = tx.next_sendable_frame() {
            let mut blocked = false;
            let sent = frame.send_blocking(|data| match this.socket.get_ref().send(data) {
                Ok(n) => Ok(n),
                Err(e) if is_transient(&e) => {
                    blocked = true;
                    Ok(0)
                }
                Err(e) => {
                    tracing::error!("sending PDU failed: {e}");
                    Err(EtherCrabError::SendFrame)
                }
            });
            // `send_blocking` turns the `Ok(0)` above into `Err(PartialSend)` after handing
            // the frame back, so `blocked` must be checked before `sent` is treated as fatal.
            // The interface queue drains within microseconds and a down link reports
            // `ENETDOWN` rather than `ENOBUFS`, so respinning beats arming a coarse timer.
            if blocked {
                cx.waker().wake_by_ref();
                break;
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
                    let records = BpfRecords {
                        buf: &this.buf[..n],
                        offset: 0,
                    };
                    for frame in records {
                        if let Err(e) = rx.receive_frame(frame) {
                            tracing::trace!("skipping unprocessable RX frame: {e}");
                        }
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
                // Readiness was cleared; the next `poll_read_ready` registers our waker.
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
    let buffer_len = socket.buffer_len;
    tracing::debug!("opening {interface} with a {buffer_len} B BPF buffer");

    Ok(TxRxFut {
        socket: AsyncFd::with_interest(socket, Interest::READABLE)?,
        buf: vec![0u8; buffer_len].into_boxed_slice(),
        tx: Some(pdu_tx),
        rx: Some(pdu_rx),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        BH_CAPLEN, BH_DATALEN, BH_HDRLEN, BPF_HDR_LEN, BpfRecords, ETHERCAT_ETHERTYPE,
        ETHERCAT_FILTER, bpf_wordalign, ifreq_for,
    };

    fn push_record(buf: &mut Vec<u8>, frame: &[u8], datalen: usize) {
        let start = buf.len();
        buf.resize(start + BPF_HDR_LEN, 0);
        let header = &mut buf[start..];
        header[BH_CAPLEN..BH_CAPLEN + 4]
            .copy_from_slice(&u32::try_from(frame.len()).unwrap().to_ne_bytes());
        header[BH_DATALEN..BH_DATALEN + 4]
            .copy_from_slice(&u32::try_from(datalen).unwrap().to_ne_bytes());
        header[BH_HDRLEN..BH_HDRLEN + 2]
            .copy_from_slice(&u16::try_from(BPF_HDR_LEN).unwrap().to_ne_bytes());
        buf.extend_from_slice(frame);
        buf.resize(bpf_wordalign(buf.len()), 0);
    }

    #[test]
    fn a_single_read_yields_every_frame_it_carries() {
        let mut buf = Vec::new();
        push_record(&mut buf, &[0xaa; 60], 60);
        push_record(&mut buf, &[0xbb; 61], 61);
        push_record(&mut buf, &[0xcc; 60], 60);

        let frames: Vec<&[u8]> = BpfRecords {
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
        assert_eq!(bpf_wordalign(BPF_HDR_LEN + 61), tail);

        let frames: Vec<&[u8]> = BpfRecords {
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

        let frames: Vec<&[u8]> = BpfRecords {
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

        let frames: Vec<&[u8]> = BpfRecords {
            buf: &buf[..complete + BPF_HDR_LEN + 10],
            offset: 0,
        }
        .collect();
        assert_eq!(frames, [&[0xaa; 60][..]]);
    }

    #[test]
    fn an_empty_read_yields_nothing() {
        assert_eq!(
            BpfRecords {
                buf: &[],
                offset: 0
            }
            .count(),
            0
        );
    }

    #[test]
    fn the_filter_matches_the_ethertype_at_the_ethernet_header_offset() {
        assert_eq!(ETHERCAT_FILTER[0].k, 12);
        assert_eq!(ETHERCAT_FILTER[1].k, ETHERCAT_ETHERTYPE);
        assert_eq!(ETHERCAT_ETHERTYPE.to_be_bytes()[2..], [0x88, 0xa4]);
        assert_eq!(ETHERCAT_FILTER[2].k, u32::MAX);
        assert_eq!(ETHERCAT_FILTER[3].k, 0);
    }

    #[test]
    fn ifreq_for_copies_a_nul_terminated_name() {
        let ifreq = ifreq_for("en0").expect("short name");
        let name: Vec<u8> = ifreq
            .ifr_name
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| c.to_ne_bytes()[0])
            .collect();
        assert_eq!(name, b"en0");
        assert_eq!(ifreq.ifr_name[libc::IF_NAMESIZE - 1], 0);
    }

    #[test]
    fn ifreq_for_rejects_a_name_that_leaves_no_room_for_the_nul() {
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE - 1)).is_ok());
        assert!(ifreq_for(&"e".repeat(libc::IF_NAMESIZE)).is_err());
    }
}

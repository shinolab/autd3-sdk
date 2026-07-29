use std::io;
use std::time::{Duration, Instant};

use pcap::{Active, Capture, Device, Direction};

use super::RawBus;
use crate::wire::ETHERTYPE_ETHERCAT;

const MTU: usize = 1500;
const READ_TIMEOUT_MS: i32 = 1;
const CAPTURE_BUFFER_BYTES: i32 = 1 << 20;

fn to_io(error: &pcap::Error) -> io::Error {
    match error {
        pcap::Error::IoError(kind) => io::Error::from(*kind),
        other => io::Error::other(other.to_string()),
    }
}

pub fn interface_candidates() -> io::Result<Vec<String>> {
    Ok(Device::list()
        .map_err(|e| to_io(&e))?
        .into_iter()
        .filter(|device| !device.flags.is_loopback() && !device.flags.is_wireless())
        .map(|device| device.name)
        .collect())
}

pub struct RawSocket {
    capture: Capture<Active>,
}

impl RawSocket {
    pub fn open(interface: &str) -> io::Result<Self> {
        let mut capture = Capture::from_device(interface)
            .map_err(|e| to_io(&e))?
            .promisc(true)
            .immediate_mode(true)
            .buffer_size(CAPTURE_BUFFER_BYTES)
            .timeout(READ_TIMEOUT_MS)
            .open()
            .map_err(|e| to_io(&e))?;

        if let Err(e) = capture.direction(Direction::In) {
            tracing::warn!(
                interface,
                "the driver refused an inbound-only capture; sent frames are rejected by \
                 comparison instead: {e}"
            );
        }
        capture
            .filter(&format!("ether proto {ETHERTYPE_ETHERCAT:#06x}"), true)
            .map_err(|e| to_io(&e))?;

        Ok(Self { capture })
    }
}

impl RawBus for RawSocket {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.capture.sendpacket(frame).map_err(|e| to_io(&e))
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.capture.next_packet() {
                Ok(packet) => {
                    let len = packet.data.len().min(buf.len());
                    buf[..len].copy_from_slice(&packet.data[..len]);
                    return Ok(Some(len));
                }
                Err(pcap::Error::TimeoutExpired | pcap::Error::NoMorePackets) => {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(to_io(&e)),
            }
        }
    }

    fn mtu(&self) -> usize {
        MTU
    }
}

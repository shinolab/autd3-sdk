use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;

use autd3_rs_link_echocat::bus::RawBus;
use pcap_file::pcap::{PcapPacket, PcapWriter};

pub const DEFAULT_STEP_NS: u64 = 1_000;

pub struct PcapTap<B> {
    inner: B,
    writer: PcapWriter<File>,
    clock_ns: u64,
    step_ns: u64,
}

fn to_io(error: &pcap_file::PcapError) -> io::Error {
    io::Error::other(error.to_string())
}

impl<B: RawBus> PcapTap<B> {
    pub fn new(inner: B, path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::create(path)?;
        let writer = PcapWriter::new(file).map_err(|e| to_io(&e))?;
        Ok(Self {
            inner,
            writer,
            clock_ns: 0,
            step_ns: DEFAULT_STEP_NS,
        })
    }

    #[must_use]
    pub fn with_step_ns(mut self, step_ns: u64) -> Self {
        self.step_ns = step_ns;
        self
    }

    pub fn into_inner(self) -> B {
        self.inner
    }

    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }

    fn record(&mut self, frame: &[u8]) -> io::Result<()> {
        self.clock_ns = self.clock_ns.saturating_add(self.step_ns);
        let len = u32::try_from(frame.len()).unwrap_or(u32::MAX);
        self.writer
            .write_packet(&PcapPacket::new(
                Duration::from_nanos(self.clock_ns),
                len,
                frame,
            ))
            .map_err(|e| to_io(&e))?;
        Ok(())
    }
}

impl<B: RawBus> RawBus for PcapTap<B> {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.record(frame)?;
        self.inner.send(frame)
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let received = self.inner.receive(buf, timeout)?;
        if let Some(len) = received {
            self.record(&buf[..len])?;
        }
        Ok(received)
    }

    fn mtu(&self) -> usize {
        self.inner.mtu()
    }
}

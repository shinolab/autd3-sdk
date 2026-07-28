mod sii;
mod subdevice;

pub use sii::{Identity, SiiImage};
pub use subdevice::{SmConfig, SubDevice};

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use crate::bus::RawBus;
use crate::reg;
use crate::wire::{
    Command, DATAGRAM_HEADER_BYTES, FRAME_HEADER_BYTES, LOCALLY_ADMINISTERED_BIT,
    SOURCE_MAC_OFFSET, WKC_BYTES,
};

pub const AUTD3_IDENTITY: Identity = Identity {
    vendor: 0x0000_08a9,
    product: 0x0000_0001,
    revision: 0x0000_0001,
    serial: 0,
};

pub const DEFAULT_HOP_NS: u64 = 300;

pub trait ProcessData: Send {
    fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]);
}

pub struct NopProcessData;

impl ProcessData for NopProcessData {
    fn exchange(&mut self, _outputs: &[u8], _inputs: &mut [u8]) {}
}

pub struct EscSim {
    devices: Vec<SubDevice>,
    now_ns: u64,
    hop_ns: u64,
    cycle_ns: u64,
    pending: VecDeque<Vec<u8>>,
    mtu: usize,
    dropped_frames: usize,
}

impl EscSim {
    #[must_use]
    pub fn new(mut devices: Vec<SubDevice>, cycle: Duration) -> Self {
        let last = devices.len().saturating_sub(1);
        for (index, device) in devices.iter_mut().enumerate() {
            device.set_port1_link(index != last);
        }
        Self {
            devices,
            now_ns: 1_000_000,
            hop_ns: DEFAULT_HOP_NS,
            cycle_ns: u64::try_from(cycle.as_nanos()).expect("cycle fits in u64 nanoseconds"),
            pending: VecDeque::new(),
            mtu: 1500,
            dropped_frames: 0,
        }
    }

    #[must_use]
    pub fn with_process_data<F>(count: usize, cycle: Duration, mut factory: F) -> Self
    where
        F: FnMut(usize) -> Box<dyn ProcessData>,
    {
        let devices = (0..count)
            .map(|index| {
                SubDevice::new(
                    u16::try_from(index).expect("device index fits in u16"),
                    AUTD3_IDENTITY,
                    i64::try_from(index).expect("device index fits in i64") * 12_345,
                    factory(index),
                )
            })
            .collect();
        Self::new(devices, cycle)
    }

    #[must_use]
    pub fn nop(count: usize, cycle: Duration) -> Self {
        Self::with_process_data(count, cycle, |_| Box::new(NopProcessData))
    }

    #[must_use]
    pub fn devices(&self) -> &[SubDevice] {
        &self.devices
    }

    #[must_use]
    pub fn now_ns(&self) -> u64 {
        self.now_ns
    }

    pub fn drop_next_frames(&mut self, count: usize) {
        self.dropped_frames = count;
    }

    pub fn latch_al_error(&mut self, state: reg::AlState, code: u16) {
        for device in &mut self.devices {
            device.latch_al_error(state, code);
        }
    }

    fn advance_cycle(&mut self) {
        self.now_ns = self.now_ns.wrapping_add(self.cycle_ns);
        let now = self.now_ns;
        for device in &mut self.devices {
            device.sync0(now);
        }
    }

    fn latch_port_times(&mut self) {
        let count = u64::try_from(self.devices.len()).expect("device count fits in u64");
        let now = self.now_ns;
        let hop = self.hop_ns;
        for (index, device) in self.devices.iter_mut().enumerate() {
            let index = u64::try_from(index).expect("device index fits in u64");
            let port0 = now + index * hop;
            let port1 = now + (2 * (count - 1) - index) * hop;
            device.latch_port_times(port0, port1);
        }
    }

    fn scan(frame: &[u8]) -> (bool, bool) {
        let mut advances = false;
        let mut latches = false;
        let mut at = FRAME_HEADER_BYTES;
        while at + DATAGRAM_HEADER_BYTES + WKC_BYTES <= frame.len() {
            let command = Command::from_code(frame[at]);
            let register = u16::from_le_bytes([frame[at + 4], frame[at + 5]]);
            let length_field = u16::from_le_bytes([frame[at + 6], frame[at + 7]]);
            let len = usize::from(length_field & 0x07ff);
            match command {
                Some(Command::Frmw | Command::Armw) if register == reg::DC_SYSTEM_TIME => {
                    advances = true;
                }
                Some(Command::Bwr) if register == reg::DC_RECEIVE_TIME_PORT0 => latches = true,
                _ => {}
            }
            at += DATAGRAM_HEADER_BYTES + len + WKC_BYTES;
            if length_field & 0x8000 == 0 {
                break;
            }
        }
        (advances, latches)
    }

    fn process(&mut self, frame: &mut [u8]) {
        let (advances, latches) = Self::scan(frame);
        if advances {
            self.advance_cycle();
        }
        if latches {
            self.latch_port_times();
        }

        let now = self.now_ns;
        let mut at = FRAME_HEADER_BYTES;
        while at + DATAGRAM_HEADER_BYTES + WKC_BYTES <= frame.len() {
            let Some(command) = Command::from_code(frame[at]) else {
                break;
            };
            let length_field = u16::from_le_bytes([frame[at + 6], frame[at + 7]]);
            let len = usize::from(length_field & 0x07ff);
            let data_at = at + DATAGRAM_HEADER_BYTES;
            if data_at + len + WKC_BYTES > frame.len() {
                break;
            }

            let mut address = [frame[at + 2], frame[at + 3], frame[at + 4], frame[at + 5]];
            let mut wkc = 0u16;
            let (head, tail) = frame.split_at_mut(data_at);
            let data = &mut tail[..len];
            for (index, device) in self.devices.iter_mut().enumerate() {
                device.handle(command, &mut address, data, &mut wkc, now, index == 0);
            }
            head[at + 2..at + 6].copy_from_slice(&address);
            frame[data_at + len..data_at + len + WKC_BYTES].copy_from_slice(&wkc.to_le_bytes());

            at = data_at + len + WKC_BYTES;
            if length_field & 0x8000 == 0 {
                break;
            }
        }

        if frame.len() > SOURCE_MAC_OFFSET
            && self
                .devices
                .first()
                .is_some_and(SubDevice::destroys_non_ethercat_frames)
        {
            frame[SOURCE_MAC_OFFSET] |= LOCALLY_ADMINISTERED_BIT;
        }
    }
}

impl RawBus for EscSim {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        if self.dropped_frames > 0 {
            self.dropped_frames -= 1;
            return Ok(());
        }
        let mut response = frame.to_vec();
        self.process(&mut response);
        self.pending.push_back(response);
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], _timeout: Duration) -> io::Result<Option<usize>> {
        let Some(frame) = self.pending.pop_front() else {
            return Ok(None);
        };
        if frame.len() > buf.len() {
            return Err(io::Error::other(
                "simulated frame exceeds the receive buffer",
            ));
        }
        buf[..frame.len()].copy_from_slice(&frame);
        Ok(Some(frame.len()))
    }

    fn mtu(&self) -> usize {
        self.mtu
    }
}

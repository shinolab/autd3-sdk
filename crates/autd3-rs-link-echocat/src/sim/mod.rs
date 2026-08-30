mod sii;
mod subdevice;

pub use sii::{Identity, SiiImage};
pub use subdevice::{SmConfig, SubDevice};

use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

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

pub use crate::master::budget::{DEFAULT_HOP_NS, DEFAULT_LINK_SPEED_MBPS, WireTiming};

const MAX_CATCHUP_EDGES: u64 = 1024;
const SPIN_THRESHOLD: Duration = Duration::from_micros(200);

fn wait_until(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if let Some(coarse) = remaining.checked_sub(SPIN_THRESHOLD) {
        std::thread::sleep(coarse);
    }
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

struct Wire {
    timing: WireTiming,
    base: Instant,
    origin_ns: u64,
    free_at: Instant,
    next_edge_ns: u64,
}

struct Pending {
    frame: Vec<u8>,
    ready_at: Option<Instant>,
    processed: bool,
}

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
    pending: VecDeque<Pending>,
    mtu: usize,
    dropped_frames: usize,
    wire: Option<Wire>,
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
            wire: None,
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

    pub fn set_wire_timing(&mut self, timing: Option<WireTiming>) {
        self.wire = timing.map(|timing| {
            let base = Instant::now();
            Wire {
                timing,
                base,
                origin_ns: self.now_ns,
                free_at: base,
                next_edge_ns: self.now_ns.saturating_add(self.cycle_ns),
            }
        });
    }

    #[must_use]
    pub fn wire_timing(&self) -> Option<WireTiming> {
        self.wire.as_ref().map(|wire| wire.timing)
    }

    fn pump_time(&mut self) {
        let (base, origin_ns, mut edge) = match self.wire.as_ref() {
            Some(wire) => (wire.base, wire.origin_ns, wire.next_edge_ns),
            None => return,
        };
        if self.cycle_ns == 0 {
            return;
        }
        let elapsed = u64::try_from(base.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let target = origin_ns.saturating_add(elapsed);
        edge = edge.max(target.saturating_sub(self.cycle_ns.saturating_mul(MAX_CATCHUP_EDGES)));
        while edge <= target {
            self.now_ns = edge;
            for device in &mut self.devices {
                device.sync0(edge);
            }
            edge = edge.saturating_add(self.cycle_ns);
        }
        self.now_ns = target;
        if let Some(wire) = self.wire.as_mut() {
            wire.next_edge_ns = edge;
        }
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
        if advances && self.wire.is_none() {
            self.advance_cycle();
        }
        if latches {
            self.latch_port_times();
        }

        let now = self.now_ns;
        let hop = self.hop_ns;
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
                let arrival = now + u64::try_from(index).expect("device index fits in u64") * hop;
                device.handle(command, &mut address, data, &mut wkc, arrival, index == 0);
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
        self.pump_time();
        let devices = self.devices.len();
        let Some(wire) = self.wire.as_mut() else {
            let mut response = frame.to_vec();
            self.process(&mut response);
            self.pending.push_back(Pending {
                frame: response,
                ready_at: None,
                processed: true,
            });
            return Ok(());
        };
        let done = wire.free_at.max(Instant::now()) + wire.timing.transmit(frame.len());
        wire.free_at = done;
        let ready_at = done + wire.timing.propagation(devices);
        self.pending.push_back(Pending {
            frame: frame.to_vec(),
            ready_at: Some(ready_at),
            processed: false,
        });
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        self.pump_time();
        let Some(ready_at) = self.pending.front().map(|pending| pending.ready_at) else {
            return Ok(None);
        };
        if let Some(ready_at) = ready_at {
            let now = Instant::now();
            if ready_at.saturating_duration_since(now) > timeout {
                wait_until(now + timeout);
                self.pump_time();
                return Ok(None);
            }
            wait_until(ready_at);
            self.pump_time();
        }
        let mut pending = self
            .pending
            .pop_front()
            .expect("a pending frame was just observed");
        if !pending.processed {
            self.process(&mut pending.frame);
        }
        if pending.frame.len() > buf.len() {
            return Err(io::Error::other(
                "simulated frame exceeds the receive buffer",
            ));
        }
        buf[..pending.frame.len()].copy_from_slice(&pending.frame);
        Ok(Some(pending.frame.len()))
    }

    fn mtu(&self) -> usize {
        self.mtu
    }
}

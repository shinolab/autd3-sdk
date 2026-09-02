use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_echocat::master::init::{
    INPUT_BYTES, INPUT_LOGICAL_BASE, OUTPUT_BYTES, OUTPUT_LOGICAL_BASE,
};
use autd3_rs_link_echocat::reg;
use autd3_rs_link_echocat::wire::Command;

use crate::capture::CapturedFrame;
use crate::ecat::{self, Datagram, Direction};
use crate::error::TraceError;

const _: () = assert!(TX_FRAME_BYTES == OUTPUT_BYTES as usize);
const _: () = assert!(RX_FRAME_BYTES == INPUT_BYTES as usize);

pub const MAX_DEVICES: usize = 128;

#[derive(Clone, Debug)]
pub struct CycleRecord {
    pub timestamp_ns: u64,
    pub tx: Vec<[u8; TX_FRAME_BYTES]>,
    pub rx: Vec<[u8; RX_FRAME_BYTES]>,
    pub dc_system_time: u64,
    pub al_status: u16,
    pub inputs_wkc: u16,
    pub outgoing_frames: usize,
    pub responded_frames: usize,
    pub tx_bytes_covered: usize,
}

impl CycleRecord {
    #[must_use]
    pub fn responded(&self) -> bool {
        self.outgoing_frames > 0 && self.responded_frames >= self.outgoing_frames
    }

    #[must_use]
    pub fn rx_valid(&self) -> bool {
        self.responded() && usize::from(self.inputs_wkc) == self.rx.len()
    }

    #[must_use]
    pub fn tx_complete(&self) -> bool {
        self.tx_bytes_covered == self.tx.len() * TX_FRAME_BYTES
    }
}

pub const MAX_ACK_LAG: usize = 6;

#[derive(Clone, Debug)]
pub struct Trace {
    pub num_devices: usize,
    pub started_ns: u64,
    pub cycles: Vec<CycleRecord>,
    pub non_ethercat_frames: usize,
    pub ack_lag: usize,
}

fn infer_ack_lag(cycles: &[CycleRecord]) -> usize {
    let mut votes = [0usize; MAX_ACK_LAG + 1];
    for index in 1..cycles.len() {
        let (Some(head), Some(previous)) = (cycles[index].tx.first(), cycles[index - 1].tx.first())
        else {
            continue;
        };
        if head[0] == previous[0] {
            continue;
        }
        for (lag, vote) in votes.iter_mut().enumerate().skip(1) {
            let Some(target) = cycles.get(index + lag) else {
                continue;
            };
            if !target.rx_valid() {
                continue;
            }
            if target.rx.iter().all(|rx| rx[0] == head[0]) {
                *vote += 1;
                break;
            }
        }
    }
    let mut best = (1usize, 0usize);
    for (lag, count) in votes.iter().enumerate().skip(1) {
        if *count > best.1 {
            best = (lag, *count);
        }
    }
    best.0
}

impl Trace {
    #[must_use]
    pub fn ack_for(&self, cycle: usize) -> Option<&[[u8; RX_FRAME_BYTES]]> {
        self.cycles
            .get(cycle + self.ack_lag)
            .filter(|record| record.rx_valid())
            .map(|record| record.rx.as_slice())
    }

    #[must_use]
    pub fn nominal_period_ns(&self) -> Option<u64> {
        if self.cycles.len() < 2 {
            return None;
        }
        let mut gaps = self
            .cycles
            .windows(2)
            .map(|w| w[1].timestamp_ns.saturating_sub(w[0].timestamp_ns))
            .collect::<Vec<_>>();
        gaps.sort_unstable();
        Some(gaps[gaps.len() / 2])
    }
}

fn carries_dc_time(datagram: &Datagram<'_>) -> bool {
    datagram.command == Command::Frmw && datagram.register() == reg::DC_SYSTEM_TIME
}

fn carries_process_inputs(datagram: &Datagram<'_>) -> bool {
    datagram.command == Command::Lrd && datagram.address == INPUT_LOGICAL_BASE
}

fn opens_cycle(frame: &ecat::EtherCatFrame<'_>) -> bool {
    frame.datagrams.first().is_some_and(carries_dc_time)
        && frame.datagrams.iter().any(carries_process_inputs)
}

fn output_offset(address: u32, num_devices: usize) -> Option<usize> {
    let offset = usize::try_from(address.checked_sub(OUTPUT_LOGICAL_BASE)?).ok()?;
    (offset < num_devices * TX_FRAME_BYTES).then_some(offset)
}

fn carries_process_outputs(datagram: &Datagram<'_>, num_devices: usize) -> bool {
    datagram.command == Command::Lwr && output_offset(datagram.address, num_devices).is_some()
}

fn belongs_to_cycle(frame: &ecat::EtherCatFrame<'_>, num_devices: usize) -> bool {
    frame.datagrams.iter().any(|datagram| {
        carries_process_inputs(datagram) || carries_process_outputs(datagram, num_devices)
    })
}

fn infer_num_devices(frames: &[CapturedFrame]) -> Result<usize, TraceError> {
    for frame in frames {
        let Some(parsed) = ecat::parse(&frame.data) else {
            continue;
        };
        for datagram in &parsed.datagrams {
            if datagram.command == Command::Lrd && datagram.address == INPUT_LOGICAL_BASE {
                let devices = datagram.data.len() / usize::from(INPUT_BYTES);
                if devices == 0 {
                    continue;
                }
                if devices > MAX_DEVICES {
                    return Err(TraceError::TooManyDevices {
                        found: devices,
                        max: MAX_DEVICES,
                    });
                }
                return Ok(devices);
            }
        }
    }
    Err(TraceError::NoProcessData {
        expected: INPUT_LOGICAL_BASE,
    })
}

struct Partial {
    timestamp_ns: u64,
    num_devices: usize,
    tx_flat: Vec<u8>,
    tx_covered: Vec<bool>,
    rx_flat: Vec<u8>,
    dc_system_time: u64,
    al_status: u16,
    inputs_wkc: u16,
    outgoing: Vec<u8>,
    responded: Vec<u8>,
}

impl Partial {
    fn new(timestamp_ns: u64, num_devices: usize) -> Self {
        Self {
            timestamp_ns,
            num_devices,
            tx_flat: vec![0u8; num_devices * TX_FRAME_BYTES],
            tx_covered: vec![false; num_devices * TX_FRAME_BYTES],
            rx_flat: vec![0u8; num_devices * RX_FRAME_BYTES],
            dc_system_time: 0,
            al_status: 0,
            inputs_wkc: 0,
            outgoing: Vec::new(),
            responded: Vec::new(),
        }
    }

    fn absorb(&mut self, frame: &ecat::EtherCatFrame<'_>) {
        match frame.direction {
            Direction::Outgoing => {
                if !self.outgoing.contains(&frame.index) {
                    self.outgoing.push(frame.index);
                }
            }
            Direction::Response => {
                if !self.responded.contains(&frame.index) {
                    self.responded.push(frame.index);
                }
            }
        }
        for datagram in &frame.datagrams {
            match datagram.command {
                Command::Lwr => {
                    let Some(offset) = output_offset(datagram.address, self.num_devices) else {
                        continue;
                    };
                    let end = offset + datagram.data.len();
                    if end > self.tx_flat.len() {
                        continue;
                    }
                    self.tx_flat[offset..end].copy_from_slice(datagram.data);
                    self.tx_covered[offset..end].fill(true);
                }
                Command::Lrd if datagram.address == INPUT_LOGICAL_BASE => {
                    if frame.direction == Direction::Response
                        && datagram.data.len() == self.rx_flat.len()
                    {
                        self.rx_flat.copy_from_slice(datagram.data);
                        self.inputs_wkc = datagram.wkc;
                    }
                }
                Command::Frmw if carries_dc_time(datagram) => {
                    if frame.direction == Direction::Response && datagram.data.len() == 8 {
                        self.dc_system_time = u64::from_le_bytes(
                            datagram.data.try_into().expect("eight bytes of DC time"),
                        );
                    }
                }
                Command::Brd
                    if datagram.register() == reg::AL_STATUS
                        && frame.direction == Direction::Response
                        && datagram.data.len() >= 2 =>
                {
                    self.al_status = u16::from_le_bytes([datagram.data[0], datagram.data[1]]);
                }
                _ => {}
            }
        }
    }

    fn finish(self) -> CycleRecord {
        let tx = self.tx_flat.as_chunks::<TX_FRAME_BYTES>().0.to_vec();
        let rx = self.rx_flat.as_chunks::<RX_FRAME_BYTES>().0.to_vec();
        CycleRecord {
            timestamp_ns: self.timestamp_ns,
            tx,
            rx,
            dc_system_time: self.dc_system_time,
            al_status: self.al_status,
            inputs_wkc: self.inputs_wkc,
            outgoing_frames: self.outgoing.len(),
            responded_frames: self.responded.len(),
            tx_bytes_covered: self.tx_covered.iter().filter(|covered| **covered).count(),
        }
    }
}

pub fn assemble(frames: &[CapturedFrame]) -> Result<Trace, TraceError> {
    let num_devices = infer_num_devices(frames)?;
    let mut cycles = Vec::new();
    let mut current: Option<Partial> = None;
    let mut non_ethercat_frames = 0usize;

    for frame in frames {
        let Some(parsed) = ecat::parse(&frame.data) else {
            non_ethercat_frames += 1;
            continue;
        };
        let opens = opens_cycle(&parsed);
        if opens && parsed.direction == Direction::Outgoing {
            if let Some(partial) = current.take() {
                cycles.push(partial.finish());
            }
            current = Some(Partial::new(frame.timestamp_ns, num_devices));
        } else if current.is_none() {
            if !opens {
                continue;
            }
            current = Some(Partial::new(frame.timestamp_ns, num_devices));
        }
        if !belongs_to_cycle(&parsed, num_devices) {
            continue;
        }
        if let Some(partial) = current.as_mut() {
            partial.absorb(&parsed);
        }
    }
    if let Some(partial) = current.take() {
        cycles.push(partial.finish());
    }
    if cycles.is_empty() {
        return Err(TraceError::NoEtherCatFrames);
    }

    let started_ns = cycles[0].timestamp_ns;
    let ack_lag = infer_ack_lag(&cycles);
    Ok(Trace {
        num_devices,
        started_ns,
        cycles,
        non_ethercat_frames,
        ack_lag,
    })
}

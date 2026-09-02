use autd3_cpu_wire::Cmd;
use autd3_rs_core::protocol::TX_FRAME_BYTES;

use crate::cycle::Trace;

#[derive(Clone, Debug)]
pub struct DecodedCycle {
    pub index: usize,
    pub timestamp_ns: u64,
    pub seq: u8,
    pub raw_cmd: u8,
    pub cmd: Option<Cmd>,
    pub staged_uniformly: bool,
    pub acks: Vec<u8>,
    pub responded: bool,
    pub rx_valid: bool,
    pub al_status: u16,
    pub dc_system_time: u64,
}

impl DecodedCycle {
    #[must_use]
    pub fn acked_by_every_device(&self) -> bool {
        !self.acks.is_empty() && self.acks.iter().all(|ack| *ack == self.seq)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unacknowledged {
    pub device: usize,
    pub from_cycle: usize,
    pub to_cycle: usize,
    pub seq: u8,
    pub raw_cmd: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedRun {
    pub from_cycle: usize,
    pub to_cycle: usize,
    pub seq: u8,
    pub raw_cmd: u8,
}

impl StagedRun {
    #[must_use]
    pub const fn cycles(&self) -> usize {
        self.to_cycle - self.from_cycle + 1
    }
}

#[derive(Clone, Debug, Default)]
pub struct Decoded {
    pub cycles: Vec<DecodedCycle>,
    pub held_frames: Vec<StagedRun>,
    pub unacknowledged: Vec<Unacknowledged>,
    pub resets: usize,
    pub unknown_commands: usize,
    pub cycles_without_response: usize,
}

fn decode_cycles(trace: &Trace) -> Vec<DecodedCycle> {
    trace
        .cycles
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let head = record.tx.first().copied().unwrap_or([0u8; TX_FRAME_BYTES]);
            let seq = head[0];
            let raw_cmd = head[1];
            DecodedCycle {
                index,
                timestamp_ns: record.timestamp_ns,
                seq,
                raw_cmd,
                cmd: Cmd::from_u8(raw_cmd),
                staged_uniformly: record
                    .tx
                    .iter()
                    .all(|frame| frame[0] == seq && frame[1] == raw_cmd),
                acks: trace
                    .ack_for(index)
                    .map(|rx| rx.iter().map(|frame| frame[0]).collect())
                    .unwrap_or_default(),
                responded: record.responded(),
                rx_valid: record.rx_valid(),
                al_status: record.al_status,
                dc_system_time: record.dc_system_time,
            }
        })
        .collect()
}

fn find_held_frames(cycles: &[DecodedCycle]) -> Vec<StagedRun> {
    let mut runs = Vec::new();
    let mut start = 0usize;
    for index in 1..=cycles.len() {
        let same = index < cycles.len()
            && cycles[index].seq == cycles[start].seq
            && cycles[index].raw_cmd == cycles[start].raw_cmd;
        if same {
            continue;
        }
        if index - start > 1 {
            runs.push(StagedRun {
                from_cycle: cycles[start].index,
                to_cycle: cycles[index - 1].index,
                seq: cycles[start].seq,
                raw_cmd: cycles[start].raw_cmd,
            });
        }
        start = index;
    }
    runs
}

fn find_unacknowledged(cycles: &[DecodedCycle], num_devices: usize) -> Vec<Unacknowledged> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for index in 1..=cycles.len() {
        let same = index < cycles.len()
            && cycles[index].seq == cycles[start].seq
            && cycles[index].raw_cmd == cycles[start].raw_cmd;
        if same {
            continue;
        }
        let run = &cycles[start..index];
        if run[0].cmd != Some(Cmd::Reset) {
            for device in 0..num_devices {
                let mut observed = false;
                let mut acked = false;
                for cycle in run {
                    if let Some(ack) = cycle.acks.get(device) {
                        observed = true;
                        if *ack == cycle.seq {
                            acked = true;
                            break;
                        }
                    }
                }
                if observed && !acked {
                    out.push(Unacknowledged {
                        device,
                        from_cycle: run[0].index,
                        to_cycle: run[run.len() - 1].index,
                        seq: run[0].seq,
                        raw_cmd: run[0].raw_cmd,
                    });
                }
            }
        }
        start = index;
    }
    out.sort_unstable_by_key(|entry| (entry.from_cycle, entry.device));
    out
}

#[must_use]
pub fn decode(trace: &Trace) -> Decoded {
    let cycles = decode_cycles(trace);
    let mut held_frames = find_held_frames(&cycles);
    held_frames.sort_unstable_by_key(|run| (std::cmp::Reverse(run.cycles()), run.from_cycle));
    let unacknowledged = find_unacknowledged(&cycles, trace.num_devices);
    let resets = cycles
        .iter()
        .filter(|cycle| cycle.cmd == Some(Cmd::Reset))
        .count();
    let unknown_commands = cycles.iter().filter(|cycle| cycle.cmd.is_none()).count();
    let cycles_without_response = cycles.iter().filter(|cycle| !cycle.responded).count();
    Decoded {
        cycles,
        held_frames,
        unacknowledged,
        resets,
        unknown_commands,
        cycles_without_response,
    }
}

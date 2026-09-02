use autd3_cpu_wire::Cmd;
use autd3_rs_core::link::Link;
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_firmware_emulator::Audit;

use crate::cycle::Trace;

pub const DEFAULT_NUM_TRANSDUCERS: usize = 249;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayDiff {
    pub cycle: usize,
    pub device: usize,
    pub raw_cmd: u8,
    pub captured: [u8; RX_FRAME_BYTES],
    pub replayed: [u8; RX_FRAME_BYTES],
}

#[derive(Clone, Debug, Default)]
pub struct ReplayReport {
    pub ack_lag: usize,
    pub cycles_fed: usize,
    pub cycles_unconfirmed: usize,
    pub cycles_compared: usize,
    pub cycles_with_incomplete_tx: usize,
    pub converged_at: Option<usize>,
    pub diffs_before_convergence: usize,
    pub diffs: Vec<ReplayDiff>,
}

impl ReplayReport {
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.diffs.is_empty()
    }
}

fn first_reset(trace: &Trace) -> Option<usize> {
    trace.cycles.iter().position(|record| {
        record
            .tx
            .first()
            .is_some_and(|frame| Cmd::from_u8(frame[1]) == Some(Cmd::Reset))
    })
}

#[must_use]
pub fn replay(trace: &Trace, num_transducers: usize) -> (Audit, ReplayReport) {
    let mut audit = Audit::new(std::iter::repeat_n(num_transducers, trace.num_devices));
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; trace.num_devices];
    let converged_at = first_reset(trace);
    let mut report = ReplayReport {
        ack_lag: trace.ack_lag,
        converged_at,
        ..ReplayReport::default()
    };

    for (index, record) in trace.cycles.iter().enumerate() {
        if record.rx_valid() {
            if !record.tx_complete() {
                report.cycles_with_incomplete_tx += 1;
            }
            let tx: &[[u8; TX_FRAME_BYTES]] = &record.tx;
            audit.cycle(tx, &mut rx).expect("the emulator never fails");
            report.cycles_fed += 1;
        } else {
            report.cycles_unconfirmed += 1;
            for (device, slot) in rx.iter_mut().enumerate() {
                audit.device(device).rx().write_to(slot);
            }
        }

        let Some(next) = trace.cycles.get(index + trace.ack_lag) else {
            continue;
        };
        if !next.rx_valid() {
            continue;
        }
        report.cycles_compared += 1;
        let settled = converged_at.is_none_or(|reset| index >= reset);
        for (device, (captured, replayed)) in next.rx.iter().zip(&rx).enumerate() {
            if captured == replayed {
                continue;
            }
            if settled {
                report.diffs.push(ReplayDiff {
                    cycle: index,
                    device,
                    raw_cmd: record.tx.first().map_or(0, |frame| frame[1]),
                    captured: *captured,
                    replayed: *replayed,
                });
            } else {
                report.diffs_before_convergence += 1;
            }
        }
    }

    (audit, report)
}

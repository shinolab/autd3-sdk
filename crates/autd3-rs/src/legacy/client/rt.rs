use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use autd3_rs_core::link::{CycleOutcome, Link};
use tokio::sync::{mpsc, oneshot};

use crate::legacy::datagram::LegacyFrame;
use crate::legacy::error::{INVALID_MSG_ID, LegacyError, TimeoutPhase, check_device_error};
use crate::legacy::wire::{MsgId, RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES, TxFrame};

pub(super) type Reply = oneshot::Sender<Result<Vec<u8>, LegacyError>>;

pub(super) struct CmdMessage {
    pub(super) round: LegacyFrame,
    pub(super) reply: Reply,
}

pub(super) fn run_rt_thread<L: Link>(
    link: L,
    cmd_rx: mpsc::Receiver<CmdMessage>,
    timeout_cycles: NonZeroU32,
    closed: Arc<AtomicBool>,
    handshake_tx: oneshot::Sender<Result<(), LegacyError>>,
) {
    let mut rt = RtThread::new(link, cmd_rx, timeout_cycles, closed);
    match rt.handshake() {
        Ok(()) => {
            if handshake_tx.send(Ok(())).is_err() {
                return;
            }
        }
        Err(e) => {
            let _ = handshake_tx.send(Err(e));
            return;
        }
    }
    rt.run();
}

const HANDSHAKE_ATTEMPTS: u32 = 4;

fn unused_msg_ids(observed: &[u8]) -> Vec<MsgId> {
    let free = (0..=MsgId::MAX.get())
        .filter(|candidate| !observed.contains(candidate))
        .map(MsgId::new)
        .collect::<Vec<_>>();
    if free.is_empty() {
        tracing::warn!(
            "every message id is already in use by some device's ack; \
             the priming frame may be confused with a stale ack"
        );
        return vec![MsgId::new(0)];
    }
    free
}

struct RtThread<L: Link> {
    link: L,
    cmd_rx: mpsc::Receiver<CmdMessage>,
    timeout_cycles: NonZeroU32,
    closed: Arc<AtomicBool>,
    msg_id: MsgId,
    tx_bufs: Vec<[u8; TX_FRAME_BYTES]>,
    rx_bufs: Vec<[u8; RX_FRAME_BYTES]>,
}

impl<L: Link> RtThread<L> {
    fn new(
        link: L,
        cmd_rx: mpsc::Receiver<CmdMessage>,
        timeout_cycles: NonZeroU32,
        closed: Arc<AtomicBool>,
    ) -> Self {
        let num_devices = link.num_devices();
        let mut tx_bufs = vec![[0u8; TX_FRAME_BYTES]; num_devices];
        for buf in &mut tx_bufs {
            TxFrame::new().write_to(buf);
        }
        Self {
            link,
            cmd_rx,
            timeout_cycles,
            closed,
            msg_id: MsgId::new(0),
            tx_bufs,
            rx_bufs: vec![[0u8; RX_FRAME_BYTES]; num_devices],
        }
    }

    fn handshake(&mut self) -> Result<(), LegacyError> {
        self.cycle()?;
        let observed = self
            .rx_bufs
            .iter()
            .map(|rx| RxFrame::parse(*rx).ack.msg_id())
            .collect::<Vec<_>>();
        let candidates = unused_msg_ids(&observed);
        let budget = (self.timeout_cycles.get() / HANDSHAKE_ATTEMPTS).max(2);
        tracing::debug!(
            ?observed,
            ?candidates,
            budget,
            "priming the legacy message id"
        );

        let mut last = None;
        for candidate in candidates.into_iter().take(HANDSHAKE_ATTEMPTS as usize) {
            self.msg_id = candidate;
            let mut prime = TxFrame::new();
            prime.header.msg_id = candidate.get();
            for buf in &mut self.tx_bufs {
                prime.write_to(buf);
            }
            match self.wait_processed(TimeoutPhase::Handshake, budget) {
                Ok(()) => {
                    tracing::debug!(msg_id = candidate.get(), "primed");
                    return Ok(());
                }
                Err(e @ LegacyError::Timeout { .. }) => {
                    tracing::debug!(
                        msg_id = candidate.get(),
                        "no ack for the priming id; it may match a device's \
                         de-duplication register, trying another"
                    );
                    last = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or(LegacyError::BusNotOperational {
            phase: TimeoutPhase::Handshake,
            cycles: self.timeout_cycles.get(),
        }))
    }

    fn run(&mut self) {
        loop {
            if self.closed.load(Ordering::Acquire) {
                break;
            }
            match self.cmd_rx.try_recv() {
                Ok(msg) => {
                    let result = self.send_round(msg.round.frames());
                    let failed = result.is_err();
                    let _ = msg.reply.send(result);
                    if failed && self.closed.load(Ordering::Acquire) {
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    if let Err(e) = self.cycle() {
                        tracing::error!("link cycle failed: {e}");
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        self.cmd_rx.close();
        while let Ok(msg) = self.cmd_rx.try_recv() {
            let _ = msg.reply.send(Err(LegacyError::RtClosed));
        }
    }

    fn cycle(&mut self) -> Result<CycleOutcome, LegacyError> {
        self.link
            .cycle(&self.tx_bufs, &mut self.rx_bufs)
            .map_err(|e| LegacyError::Link(format!("link cycle failed: {e}")))
    }

    fn send_round(&mut self, round: &[TxFrame]) -> Result<Vec<u8>, LegacyError> {
        self.msg_id = self.msg_id.next();
        for (buf, frame) in self.tx_bufs.iter_mut().zip(round) {
            let mut staged = frame.clone();
            staged.header.msg_id = self.msg_id.get();
            staged.write_to(buf);
        }
        tracing::trace!(
            msg_id = self.msg_id.get(),
            tag = round.first().map(|f| f.payload[0]),
            "staged legacy frame"
        );

        let tag = round.first().map_or(0, |f| f.payload[0]);
        self.wait_processed(TimeoutPhase::Command { tag }, self.timeout_cycles.get())?;

        for (device, rx) in self.rx_bufs.iter().enumerate() {
            check_device_error(device, RxFrame::parse(*rx).ack.err())?;
        }
        Ok(self.rx_bufs.iter().map(|rx| rx[0]).collect())
    }

    fn wait_processed(&mut self, phase: TimeoutPhase, budget: u32) -> Result<(), LegacyError> {
        let mut cycles = 0u32;
        let mut stale_cycles = 0u32;
        loop {
            let CycleOutcome { rx_valid } = self.cycle()?;
            cycles += 1;
            if rx_valid {
                if self.all_processed() {
                    return Ok(());
                }
            } else {
                stale_cycles += 1;
            }
            if cycles >= budget {
                return Err(if stale_cycles == cycles {
                    LegacyError::BusNotOperational { phase, cycles }
                } else {
                    LegacyError::Timeout {
                        phase,
                        cycles,
                        expected: self.msg_id.get(),
                        acks: self.ack_summary(),
                        stale_cycles,
                    }
                });
            }
        }
    }

    fn ack_summary(&self) -> String {
        self.rx_bufs
            .iter()
            .enumerate()
            .map(|(device, rx)| {
                let ack = RxFrame::parse(*rx).ack;
                format!(
                    "[{device}] msg_id={:#04x} err={:#04x}",
                    ack.msg_id(),
                    ack.err()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn all_processed(&self) -> bool {
        self.rx_bufs.iter().all(|rx| {
            let ack = RxFrame::parse(*rx).ack;
            ack.msg_id() == self.msg_id.get() || ack.err() == INVALID_MSG_ID
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(observed: &[u8]) -> Vec<u8> {
        unused_msg_ids(observed)
            .into_iter()
            .map(MsgId::get)
            .collect()
    }

    #[test]
    fn candidate_ids_avoid_every_reported_ack() {
        assert_eq!(ids(&[0])[0], 1);
        assert_eq!(ids(&[1, 2, 3])[0], 0);
        assert_eq!(ids(&[0, 1, 2])[0], 3);
        assert!(!ids(&[0, 5, 9]).iter().any(|v| [0, 5, 9].contains(v)));
    }

    #[test]
    fn several_candidates_are_offered_so_a_swallowed_prime_can_be_retried() {
        assert!(ids(&[0]).len() >= HANDSHAKE_ATTEMPTS as usize);
    }

    #[test]
    fn a_fully_covered_id_space_falls_back_to_zero() {
        let all = (0..=MsgId::MAX.get()).collect::<Vec<_>>();
        assert_eq!(ids(&all), vec![0]);
    }
}

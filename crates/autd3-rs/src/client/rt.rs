use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{mpsc, oneshot};

use crate::error::Error;
use crate::link::{CycleOutcome, Link};
use crate::protocol::{
    Cmd, MODE_LOW_LATENCY, RX_FRAME_BYTES, RxFrame, Seq, TX_FRAME_BYTES, TxFrame,
};
use crate::response::Response;

use super::config::{ClientConfig, MAX_DEVICES};
use super::pool::{Slot, SlotPool};

pub(super) struct CmdMessage {
    pub(super) frame: Slot,
    pub(super) response_tx: oneshot::Sender<Result<Response, Error>>,
}

struct InFlight {
    seq: Seq,
    frame: Slot,
    acked: u128,
    age: u32,
    response_tx: oneshot::Sender<Result<Response, Error>>,
}

fn stage_frame(seq: Seq, frame: &Slot, tx_bufs: &mut [[u8; TX_FRAME_BYTES]]) {
    for (device, buf) in tx_bufs.iter_mut().enumerate() {
        buf[0] = seq.get();
        buf[1] = frame.cmd_for(device).as_u8();
        buf[2..].copy_from_slice(frame.payload_for(device));
    }
}

enum HeadAction {
    None,
    Reset,
    GiveUp,
}

#[derive(Default)]
struct ResyncState {
    active: bool,
    rounds: u32,
    reset_tried: bool,
}

impl ResyncState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn on_ack_progress(&mut self, pending: &mut VecDeque<InFlight>) {
        if self.active {
            self.rounds = 0;
            self.reset_tried = false;
            if let Some(head) = pending.front_mut() {
                head.age = 0;
            }
        }
    }

    fn advance_head(
        &mut self,
        pending: &mut VecDeque<InFlight>,
        config: &ClientConfig,
    ) -> HeadAction {
        let Some(head) = pending.front_mut() else {
            if self.active {
                self.reset();
            }
            return HeadAction::None;
        };
        head.age = head.age.saturating_add(1);
        if head.age < config.timeout_cycles {
            return HeadAction::None;
        }
        head.age = 0;
        if !self.active {
            self.active = true;
            self.rounds = 0;
            return HeadAction::None;
        }
        self.rounds += 1;
        if self.rounds < config.max_resync_rounds.get() {
            return HeadAction::None;
        }
        self.rounds = 0;
        if self.reset_tried {
            HeadAction::GiveUp
        } else {
            self.reset_tried = true;
            HeadAction::Reset
        }
    }
}

fn apply_thread_tuning(config: &ClientConfig) {
    if let Some(priority) = config.rt_priority
        && let Err(e) = thread_priority::set_current_thread_priority(priority)
    {
        tracing::warn!("failed to set RT thread priority: {e:?}");
    }
    if let Some(core) = config.rt_affinity
        && !core_affinity::set_for_current(core)
    {
        tracing::warn!("failed to pin RT thread to core {}", core.id);
    }
}

pub(super) fn run_rt_thread<L: Link>(
    link: L,
    cmd_rx: mpsc::Receiver<CmdMessage>,
    pool: Arc<SlotPool>,
    config: ClientConfig,
    hs_done_tx: oneshot::Sender<Result<(), String>>,
    closed: Arc<AtomicBool>,
) {
    apply_thread_tuning(&config);
    let mut rt = RtThread::new(link, cmd_rx, pool, config, closed);
    match rt.handshake() {
        Ok(()) => {}
        Err(e) => {
            let _ = hs_done_tx.send(Err(e));
            return;
        }
    }
    if hs_done_tx.send(Ok(())).is_err() {
        return;
    }
    rt.run();
}

struct RtThread<L: Link> {
    link: L,
    cmd_rx: mpsc::Receiver<CmdMessage>,
    pool: Arc<SlotPool>,
    config: ClientConfig,
    closed: Arc<AtomicBool>,

    all_acked: u128,
    tx_bufs: Vec<[u8; TX_FRAME_BYTES]>,
    rx_bufs: Vec<[u8; RX_FRAME_BYTES]>,

    next_seq: Seq,
    pending: VecDeque<InFlight>,
    cycle_idx: u64,
    next_pickup_at: u64,
    send_interval: u64,
    resync: ResyncState,
    stale_run: u32,
    reset_remaining: u32,
    stale_limit: u32,
}

enum StageOutcome {
    Staged,
    Disconnected,
}

impl<L: Link> RtThread<L> {
    fn new(
        link: L,
        cmd_rx: mpsc::Receiver<CmdMessage>,
        pool: Arc<SlotPool>,
        config: ClientConfig,
        closed: Arc<AtomicBool>,
    ) -> Self {
        let num_devices = link.num_devices();
        let all_acked: u128 = if num_devices == MAX_DEVICES {
            u128::MAX
        } else {
            (1u128 << num_devices) - 1
        };
        let stale_limit = config
            .timeout_cycles
            .saturating_mul(config.max_resync_rounds.get());
        Self {
            link,
            cmd_rx,
            pool,
            send_interval: u64::from(config.send_interval_cycles.get()),
            pending: VecDeque::with_capacity(config.max_inflight.get()),
            config,
            closed,
            all_acked,
            tx_bufs: vec![[0u8; TX_FRAME_BYTES]; num_devices],
            rx_bufs: vec![[0u8; RX_FRAME_BYTES]; num_devices],
            next_seq: Seq::ZERO,
            cycle_idx: 0,
            next_pickup_at: 0,
            resync: ResyncState::default(),
            stale_run: 0,
            reset_remaining: 0,
            stale_limit,
        }
    }

    fn handshake(&mut self) -> Result<(), String> {
        for seq in [Seq::ZERO, Seq::new(1)] {
            for buf in &mut self.tx_bufs {
                TxFrame::new(seq, Cmd::Reset).write_to(buf);
            }
            self.link
                .cycle(&self.tx_bufs, &mut self.rx_bufs)
                .map_err(|e| format!("handshake failed: {e}"))?;
        }

        self.next_seq = if self.config.low_latency {
            self.negotiate_low_latency()?
        } else {
            Seq::ZERO
        };
        Ok(())
    }

    fn negotiate_low_latency(&mut self) -> Result<Seq, String> {
        let mut frame = TxFrame::new(Seq::ZERO, Cmd::SetMode);
        frame.payload[0] = MODE_LOW_LATENCY;
        for buf in &mut self.tx_bufs {
            frame.write_to(buf);
        }

        let bound = self.config.timeout_cycles.max(2);
        for _ in 0..bound {
            let CycleOutcome { rx_valid } = self
                .link
                .cycle(&self.tx_bufs, &mut self.rx_bufs)
                .map_err(|e| format!("handshake failed: {e}"))?;
            if rx_valid && self.rx_bufs.iter().all(|rx| rx[0] == Seq::ZERO.get()) {
                return Ok(Seq::new(1));
            }
        }
        Ok(Seq::ZERO)
    }

    fn run(&mut self) {
        loop {
            if self.closed.load(Ordering::Acquire) {
                break;
            }

            if matches!(self.stage_tx(), StageOutcome::Disconnected) {
                break;
            }

            let Ok(rx_valid) = self.cycle_once() else {
                return;
            };

            if self.reset_remaining > 0 {
                self.advance_reset_phase();
            } else if rx_valid {
                self.handle_healthy();
            } else {
                self.handle_stale();
            }

            self.cycle_idx = self.cycle_idx.wrapping_add(1);
        }

        self.teardown();
    }

    fn stage_tx(&mut self) -> StageOutcome {
        if self.reset_remaining > 0 {
            for buf in &mut self.tx_bufs {
                TxFrame::new(Seq::ZERO, Cmd::Reset).write_to(buf);
            }
        } else if self.resync.active {
            if let Some(front) = self.pending.front() {
                stage_frame(front.seq, &front.frame, &mut self.tx_bufs);
            }
        } else if self.cycle_idx >= self.next_pickup_at
            && self.pending.len() < self.config.max_inflight.get()
        {
            match self.cmd_rx.try_recv() {
                Ok(msg) => {
                    let seq = self.next_seq;
                    self.next_seq = self.next_seq.next();
                    stage_frame(seq, &msg.frame, &mut self.tx_bufs);
                    self.pending.push_back(InFlight {
                        seq,
                        frame: msg.frame,
                        acked: 0,
                        age: 0,
                        response_tx: msg.response_tx,
                    });
                    self.next_pickup_at = self.cycle_idx + self.send_interval;
                }
                Err(mpsc::error::TryRecvError::Empty) => {}
                Err(mpsc::error::TryRecvError::Disconnected) => return StageOutcome::Disconnected,
            }
        }
        StageOutcome::Staged
    }

    fn cycle_once(&mut self) -> Result<bool, ()> {
        match self.link.cycle(&self.tx_bufs, &mut self.rx_bufs) {
            Ok(CycleOutcome { rx_valid }) => Ok(rx_valid),
            Err(e) => {
                let msg = format!("link cycle failed: {e}");
                for entry in self.pending.drain(..) {
                    let _ = entry.response_tx.send(Err(Error::Link(msg.clone())));
                    self.pool.release(entry.frame);
                }
                Err(())
            }
        }
    }

    fn advance_reset_phase(&mut self) {
        self.reset_remaining -= 1;
        if self.reset_remaining == 0 {
            let mut seq = Seq::ZERO;
            for entry in &mut self.pending {
                entry.seq = seq;
                entry.acked = 0;
                entry.age = 0;
                seq = seq.next();
            }
            self.next_seq = seq;
            self.resync.active = !self.pending.is_empty();
            self.resync.rounds = 0;
        }
    }

    fn handle_healthy(&mut self) {
        self.stale_run = 0;
        self.route_acks();
        match self.resync.advance_head(&mut self.pending, &self.config) {
            HeadAction::None => {}
            HeadAction::Reset => self.reset_remaining = self.config.reset_resend_cycles,
            HeadAction::GiveUp => {
                self.fail_pending_timeout();
                self.resync.reset();
            }
        }
    }

    fn handle_stale(&mut self) {
        self.stale_run = self.stale_run.saturating_add(1);
        if self.stale_run >= self.stale_limit {
            self.fail_pending_timeout();
            self.resync.reset();
            self.stale_run = 0;
        }
    }

    fn route_acks(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let front_seq = self.pending.front().expect("non-empty").seq;
        let back_seq = self.pending.back().expect("non-empty").seq;
        let span = back_seq.distance_from(front_seq) as usize;
        for (device, rx_buf) in self.rx_bufs.iter().enumerate() {
            let rx = RxFrame::parse(rx_buf);
            let ack_offset = rx.ack.distance_from(front_seq) as usize;
            if ack_offset > span {
                continue;
            }
            let bit = 1u128 << device;
            for entry in self.pending.iter_mut().take(ack_offset + 1) {
                if entry.acked & bit == 0 {
                    entry.acked |= bit;
                    entry.frame.record_data(device, rx.data);
                }
            }
        }
        let mut progressed = false;
        while self
            .pending
            .front()
            .is_some_and(|entry| entry.acked == self.all_acked)
        {
            let entry = self.pending.pop_front().expect("just checked");
            let _ = entry.response_tx.send(Ok(Response {
                data: entry.frame.data().to_vec(),
            }));
            self.pool.release(entry.frame);
            progressed = true;
        }
        if progressed {
            self.resync.on_ack_progress(&mut self.pending);
        }
    }

    fn fail_pending_timeout(&mut self) {
        for entry in self.pending.drain(..) {
            let _ = entry.response_tx.send(Err(Error::Timeout {
                cycles: self.config.timeout_cycles,
            }));
            self.pool.release(entry.frame);
        }
    }

    fn teardown(&mut self) {
        for entry in self.pending.drain(..) {
            let _ = entry.response_tx.send(Err(Error::RtClosed));
            self.pool.release(entry.frame);
        }
        while let Ok(msg) = self.cmd_rx.try_recv() {
            let _ = msg.response_tx.send(Err(Error::RtClosed));
            self.pool.release(msg.frame);
        }
    }
}

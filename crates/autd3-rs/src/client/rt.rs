use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use autd3_rs_core::BusStats;
use autd3_rs_core::rt::oneshot;

use crate::error::{Error, NetworkCause};
use crate::protocol::{Cmd, FRAME_BYTES_MAX, Seq, TxFrame};
use crate::transport::{Bus, BusTiming};
use crate::udp::Reply;

use autd3_cpu_wire::Mode;
use autd3_cpu_wire::payload::SetModePayload;
use zerocopy::FromBytes;

use super::completion::CompletionSender;
use super::config::{ClientConfig, MAX_DEVICES};
use super::pool::Slot;

const PICKUP_POLL: Duration = Duration::from_micros(200);
const RESET_ACK: u8 = 0xFF;

pub(super) struct CmdMessage {
    pub(super) frame: Slot,
    pub(super) response_tx: CompletionSender,
    pub(super) exclusive: bool,
}

struct Inflight {
    seq: Seq,
    frame: Slot,
    acked: u128,
    sent_at: Instant,
    exclusive: bool,
    response_tx: CompletionSender,
}

#[derive(Debug, thiserror::Error)]
enum HandshakeError {
    #[error("the devices did not acknowledge Reset within {rounds} attempts")]
    ResetUnconfirmed { rounds: u32 },
    #[error("the devices did not acknowledge SetMode({mode:?}) within {rounds} attempts")]
    ModeUnconfirmed { mode: Mode, rounds: u32 },
    #[error("device {device} rejected SetMode({mode:?}) with status {status:#04x}")]
    ModeRejected {
        device: usize,
        mode: Mode,
        status: u8,
    },
}

fn stage_frame(seq: Seq, frame: &Slot, bufs: &mut [[u8; FRAME_BYTES_MAX]]) {
    for (device, buf) in bufs.iter_mut().enumerate() {
        buf[0] = seq.get();
        buf[1] = frame.cmd_for(device).as_u8();
        buf[2..].copy_from_slice(frame.payload_for(device));
    }
}

const EPOCH_TRAIL: u16 = 0x4000;

fn not_before(msg_id: u16, epoch: u16) -> bool {
    msg_id.wrapping_sub(epoch) < 0x8000
}

fn trailing_epoch(epoch: u16, latest: u16) -> u16 {
    if latest.wrapping_sub(epoch) > EPOCH_TRAIL {
        latest.wrapping_sub(EPOCH_TRAIL)
    } else {
        epoch
    }
}

enum ResetOutcome {
    Confirmed,
    Unconfirmed,
    Closed,
}

#[derive(Default)]
struct ResyncState {
    active: bool,
    rounds: u32,
    reset_tried: bool,
}

struct HeartbeatWait {
    msg_id: u16,
    deadline: Instant,
    replied: u128,
}

fn handshake_failed<E: core::error::Error + Send + Sync + 'static>(e: E) -> NetworkCause {
    tracing::error!("handshake failed: {e}");
    NetworkCause::new(e)
}

pub(super) fn run_rt_thread<L: Bus>(
    link: L,
    cmd_rx: Receiver<CmdMessage>,
    config: ClientConfig,
    hs_done_tx: oneshot::Sender<Result<(), NetworkCause>>,
    done_tx: oneshot::Sender<Option<NetworkCause>>,
    closed: Arc<AtomicBool>,
) {
    let cause = run_rt_loop(link, cmd_rx, config, hs_done_tx, closed);
    let _ = done_tx.send(cause);
}

fn run_rt_loop<L: Bus>(
    link: L,
    cmd_rx: Receiver<CmdMessage>,
    config: ClientConfig,
    hs_done_tx: oneshot::Sender<Result<(), NetworkCause>>,
    closed: Arc<AtomicBool>,
) -> Option<NetworkCause> {
    autd3_rs_core::apply_thread_tuning(autd3_rs_core::RtThreadTuning {
        priority: config.rt_priority,
        policy: config.rt_policy,
        affinity: config.rt_affinity,
    });
    let mut rt = RtThread::new(link, cmd_rx, config, closed);
    match rt.handshake() {
        Ok(()) => {}
        Err(e) => {
            let _ = hs_done_tx.send(Err(e));
            return rt.close_link();
        }
    }
    if hs_done_tx.send(Ok(())).is_err() {
        return rt.close_link();
    }
    let link_error = rt.run();
    let closed = rt.close_link();
    link_error.or(closed)
}

struct RtThread<L: Bus> {
    link: L,
    cmd_rx: Receiver<CmdMessage>,
    config: ClientConfig,
    closed: Arc<AtomicBool>,
    timing: BusTiming,
    stats: BusStats,

    all_acked: u128,
    bufs: Vec<[u8; FRAME_BYTES_MAX]>,

    next_seq: Seq,
    epoch: u16,
    pending: VecDeque<Inflight>,
    held_exclusive: Option<CmdMessage>,
    resync: ResyncState,
    head_since: Instant,
    last_send: Instant,
    heartbeat: Option<HeartbeatWait>,
}

enum Step {
    Continue,
    Disconnected,
}

impl<L: Bus> RtThread<L> {
    fn new(
        link: L,
        cmd_rx: Receiver<CmdMessage>,
        config: ClientConfig,
        closed: Arc<AtomicBool>,
    ) -> Self {
        let num_devices = link.num_devices();
        let all_acked: u128 = if num_devices == MAX_DEVICES {
            u128::MAX
        } else {
            (1u128 << num_devices) - 1
        };
        let now = Instant::now();
        Self {
            timing: link.timing(),
            stats: link.stats(),
            epoch: link.next_msg_id(),
            link,
            cmd_rx,
            pending: VecDeque::with_capacity(config.max_inflight.get()),
            held_exclusive: None,
            config,
            closed,
            all_acked,
            bufs: vec![[0u8; FRAME_BYTES_MAX]; num_devices],
            next_seq: Seq::ZERO,
            resync: ResyncState::default(),
            head_since: now,
            last_send: now,
            heartbeat: None,
        }
    }

    fn send_bufs(&mut self) -> Result<u16, NetworkCause> {
        let msg_id = self.link.send(&self.bufs).map_err(|e| {
            tracing::error!("bus send failed: {e}");
            NetworkCause::new(e)
        })?;
        self.last_send = Instant::now();
        self.epoch = trailing_epoch(self.epoch, msg_id);
        Ok(msg_id)
    }

    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, NetworkCause> {
        self.link.recv(deadline).map_err(|e| {
            tracing::error!("bus recv failed: {e}");
            NetworkCause::new(e)
        })
    }

    fn stage_all(&mut self, frame: &TxFrame) {
        for buf in &mut self.bufs {
            frame.write_to(buf);
        }
    }

    fn reset_devices(&mut self) -> Result<ResetOutcome, NetworkCause> {
        self.stage_all(&TxFrame::new(Seq::ZERO, Cmd::Reset));
        let first = self.link.next_msg_id();
        let rounds = self.config.max_resync_rounds.get();
        let mut confirmed = 0u128;
        for round in 0..rounds {
            if self.closed.load(Ordering::Acquire) {
                return Ok(ResetOutcome::Closed);
            }
            tracing::trace!(round, "sending reset");
            self.send_bufs()?;
            let deadline = Instant::now() + self.config.ack_timeout;
            while confirmed != self.all_acked {
                let Some(reply) = self.recv(deadline)? else {
                    break;
                };
                if not_before(reply.msg_id, first) && reply.ack == RESET_ACK {
                    confirmed |= 1u128 << reply.device;
                }
            }
            if confirmed == self.all_acked {
                self.epoch = self.link.next_msg_id();
                self.heartbeat = None;
                return Ok(ResetOutcome::Confirmed);
            }
        }
        Ok(ResetOutcome::Unconfirmed)
    }

    fn handshake(&mut self) -> Result<(), NetworkCause> {
        tracing::debug!(low_latency = self.config.low_latency, "starting handshake");
        let rounds = self.config.max_resync_rounds.get();
        match self.reset_devices()? {
            ResetOutcome::Confirmed => {}
            ResetOutcome::Unconfirmed | ResetOutcome::Closed => {
                return Err(handshake_failed(HandshakeError::ResetUnconfirmed {
                    rounds,
                }));
            }
        }
        self.negotiate_mode()?;
        self.next_seq = Seq::new(1);
        Ok(())
    }

    fn negotiate_mode(&mut self) -> Result<(), NetworkCause> {
        let mode = if self.config.low_latency {
            Mode::LowLatency
        } else {
            Mode::Fifo
        };
        let mut frame = TxFrame::new(Seq::ZERO, Cmd::SetMode);
        let (p, _) = SetModePayload::mut_from_prefix(&mut frame.payload).unwrap();
        p.mode = mode.as_u8();
        self.stage_all(&frame);

        let rounds = self.config.max_resync_rounds.get();
        let mut acked = 0u128;
        for _ in 0..rounds {
            self.send_bufs()?;
            let deadline = Instant::now() + self.config.ack_timeout;
            while acked != self.all_acked {
                let Some(reply) = self.recv(deadline)? else {
                    break;
                };
                if !not_before(reply.msg_id, self.epoch) || reply.ack != Seq::ZERO.get() {
                    continue;
                }
                if reply.status != 0 {
                    return Err(handshake_failed(HandshakeError::ModeRejected {
                        device: reply.device,
                        mode,
                        status: reply.status,
                    }));
                }
                acked |= 1u128 << reply.device;
            }
            if acked == self.all_acked {
                tracing::info!(?mode, "frame processing mode established");
                return Ok(());
            }
        }
        Err(handshake_failed(HandshakeError::ModeUnconfirmed {
            mode,
            rounds,
        }))
    }

    fn run(&mut self) -> Option<NetworkCause> {
        let result = self.run_loop();
        let cause = result.err();
        self.teardown(cause.as_ref());
        cause
    }

    fn run_loop(&mut self) -> Result<(), NetworkCause> {
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Ok(());
            }
            if matches!(self.pickup()?, Step::Disconnected) {
                return Ok(());
            }
            self.check_head(Instant::now())?;
            self.check_heartbeat(Instant::now())?;
            if matches!(self.wait()?, Step::Disconnected) {
                return Ok(());
            }
        }
    }

    fn window_open(&self) -> bool {
        !self.resync.active
            && self.held_exclusive.is_none()
            && !self.pending.front().is_some_and(|entry| entry.exclusive)
            && self.pending.len() < self.config.max_inflight.get()
    }

    fn pickup(&mut self) -> Result<Step, NetworkCause> {
        if self.resync.active {
            return Ok(Step::Continue);
        }
        if self.held_exclusive.is_some() {
            if !self.pending.is_empty() {
                return Ok(Step::Continue);
            }
            let msg = self.held_exclusive.take().expect("just checked is_some");
            self.stage_new(msg)?;
        }
        while self.window_open() {
            match self.cmd_rx.try_recv() {
                Ok(msg) => self.accept(msg)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(Step::Disconnected),
            }
        }
        Ok(Step::Continue)
    }

    fn accept(&mut self, msg: CmdMessage) -> Result<(), NetworkCause> {
        if msg.exclusive && !self.pending.is_empty() {
            tracing::trace!("holding exclusive frame until pending drains");
            self.held_exclusive = Some(msg);
            return Ok(());
        }
        self.stage_new(msg)
    }

    fn stage_new(&mut self, msg: CmdMessage) -> Result<(), NetworkCause> {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.next();
        tracing::trace!(
            seq = seq.get(),
            cmd = ?msg.frame.cmd_for(0),
            exclusive = msg.exclusive,
            "sending frame"
        );
        stage_frame(seq, &msg.frame, &mut self.bufs);
        let now = Instant::now();
        if self.pending.is_empty() {
            self.head_since = now;
        }
        self.pending.push_back(Inflight {
            seq,
            frame: msg.frame,
            acked: 0,
            sent_at: now,
            exclusive: msg.exclusive,
            response_tx: msg.response_tx,
        });
        self.send_bufs()?;
        self.stats.record_frame();
        Ok(())
    }

    fn retransmit_window(&mut self) -> Result<(), NetworkCause> {
        let count = self.pending.len();
        for index in 0..count {
            let entry = &self.pending[index];
            tracing::trace!(seq = entry.seq.get(), "retransmitting frame");
            stage_frame(entry.seq, &entry.frame, &mut self.bufs);
            self.send_bufs()?;
        }
        self.stats.record_retransmissions(count as u64);
        Ok(())
    }

    fn check_head(&mut self, now: Instant) -> Result<(), NetworkCause> {
        if self.pending.is_empty() {
            if self.resync.active {
                tracing::debug!("resync complete");
            }
            self.resync = ResyncState::default();
            return Ok(());
        }
        if now.saturating_duration_since(self.head_since) < self.config.ack_timeout {
            return Ok(());
        }
        self.head_since = now;
        if !self.resync.active {
            tracing::debug!(
                seq = self.pending.front().map(|e| e.seq.get()),
                "head frame unacked past the ack timeout; retransmitting the window"
            );
            self.resync.active = true;
        }
        self.resync.rounds += 1;
        if self.resync.rounds <= self.config.max_resync_rounds.get() {
            return self.retransmit_window();
        }
        self.resync.rounds = 0;
        if self.resync.reset_tried {
            tracing::warn!(
                pending = self.pending.len(),
                "sequence reset did not recover; failing pending frames with timeout"
            );
            self.fail_pending_timeout();
            self.resync = ResyncState::default();
            return Ok(());
        }
        self.resync.reset_tried = true;
        tracing::warn!("retransmissions exhausted; resetting the sequence");
        match self.reset_devices()? {
            ResetOutcome::Confirmed => {
                self.stats.record_reset();
                self.renumber_pending();
                self.head_since = Instant::now();
                self.retransmit_window()
            }
            ResetOutcome::Unconfirmed => {
                tracing::warn!(
                    pending = self.pending.len(),
                    "the devices did not acknowledge the reset; failing pending frames with timeout"
                );
                self.fail_pending_timeout();
                self.resync = ResyncState::default();
                Ok(())
            }
            ResetOutcome::Closed => Ok(()),
        }
    }

    fn renumber_pending(&mut self) {
        let mut seq = Seq::ZERO;
        for entry in &mut self.pending {
            entry.seq = seq;
            entry.acked = 0;
            seq = seq.next();
        }
        self.next_seq = seq;
        tracing::debug!(
            pending = self.pending.len(),
            "sequence reset complete; replaying pending frames"
        );
    }

    fn check_heartbeat(&mut self, now: Instant) -> Result<(), NetworkCause> {
        if let Some(wait) = &self.heartbeat
            && now >= wait.deadline
        {
            let missed = (self.all_acked & !wait.replied).count_ones();
            if missed > 0 {
                tracing::trace!(missed, "heartbeat replies missing");
                self.stats.record_missed_replies(u64::from(missed));
            }
            self.heartbeat = None;
        }
        if self.heartbeat.is_none()
            && now.saturating_duration_since(self.last_send) >= self.timing.heartbeat
        {
            let msg_id = self.link.heartbeat().map_err(|e| {
                tracing::error!("bus heartbeat failed: {e}");
                NetworkCause::new(e)
            })?;
            self.last_send = Instant::now();
            self.epoch = trailing_epoch(self.epoch, msg_id);
            self.heartbeat = Some(HeartbeatWait {
                msg_id,
                deadline: self.last_send + self.timing.reply_timeout.min(self.timing.heartbeat),
                replied: 0,
            });
        }
        Ok(())
    }

    fn next_deadline(&self, now: Instant) -> Instant {
        let mut deadline = self.last_send + self.timing.heartbeat;
        if let Some(wait) = &self.heartbeat {
            deadline = deadline.min(wait.deadline);
        }
        if !self.pending.is_empty() {
            deadline = deadline.min(self.head_since + self.config.ack_timeout);
        }
        if self.window_open() || self.held_exclusive.is_some() {
            deadline = deadline.min(now + PICKUP_POLL);
        }
        deadline.max(now)
    }

    fn wait(&mut self) -> Result<Step, NetworkCause> {
        let now = Instant::now();
        let deadline = self.next_deadline(now);
        let idle = self.pending.is_empty()
            && self.heartbeat.is_none()
            && self.held_exclusive.is_none()
            && !self.resync.active;
        if idle {
            let until = (self.last_send + self.timing.heartbeat).saturating_duration_since(now);
            let step = match self.cmd_rx.recv_timeout(until) {
                Ok(msg) => {
                    self.accept(msg)?;
                    Step::Continue
                }
                Err(RecvTimeoutError::Timeout) => Step::Continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(Step::Disconnected),
            };
            while let Some(reply) = self.recv(Instant::now())? {
                self.handle_reply(&reply);
            }
            return Ok(step);
        }
        if let Some(reply) = self.recv(deadline)? {
            self.handle_reply(&reply);
        }
        Ok(Step::Continue)
    }

    fn handle_reply(&mut self, reply: &Reply) {
        if !not_before(reply.msg_id, self.epoch) {
            tracing::trace!(
                msg_id = reply.msg_id,
                "dropping a reply from before the epoch"
            );
            return;
        }
        let bit = 1u128 << reply.device;
        if let Some(wait) = &mut self.heartbeat
            && wait.msg_id == reply.msg_id
        {
            wait.replied |= bit;
            if wait.replied == self.all_acked {
                self.heartbeat = None;
            }
        }
        self.route(reply, bit);
    }

    fn route(&mut self, reply: &Reply, bit: u128) {
        let (Some(front), Some(back)) = (self.pending.front(), self.pending.back()) else {
            return;
        };
        let front_seq = front.seq;
        let span = back.seq.distance_from(front_seq) as usize;
        let ack = Seq::new(reply.ack);
        let ack_offset = ack.distance_from(front_seq) as usize;
        if ack_offset > span {
            return;
        }
        for entry in self.pending.iter_mut().take(ack_offset + 1) {
            if entry.acked & bit != 0 {
                continue;
            }
            tracing::trace!(
                device = reply.device,
                seq = entry.seq.get(),
                "device acked frame"
            );
            entry.acked |= bit;
            if entry.seq == ack {
                entry
                    .frame
                    .record_reply(reply.device, reply.status, reply.data());
            } else {
                entry.frame.record_reply(reply.device, 0, &[]);
            }
        }
        let mut progressed = false;
        while self
            .pending
            .front()
            .is_some_and(|entry| entry.acked == self.all_acked)
        {
            let entry = self.pending.pop_front().expect("just checked");
            tracing::trace!(seq = entry.seq.get(), "frame acked by all devices");
            self.stats
                .record_ack(u64::try_from(entry.sent_at.elapsed().as_nanos()).unwrap_or(u64::MAX));
            entry.response_tx.send(Ok(entry.frame.response()));
            progressed = true;
        }
        if progressed {
            self.head_since = Instant::now();
            if self.resync.active {
                tracing::debug!("ack progress during resync");
                self.resync.rounds = 0;
                self.resync.reset_tried = false;
            }
        }
    }

    fn fail_pending_timeout(&mut self) {
        for entry in self.pending.drain(..) {
            entry.response_tx.send(Err(Error::Timeout {
                timeout: self.config.ack_timeout,
            }));
        }
    }

    fn close_link(&mut self) -> Option<NetworkCause> {
        match self.link.close() {
            Ok(()) => None,
            Err(e) => {
                tracing::error!("bus close failed: {e}");
                Some(NetworkCause::new(e))
            }
        }
    }

    fn teardown(&mut self, link_error: Option<&NetworkCause>) {
        tracing::debug!(pending = self.pending.len(), "RT thread stopping");
        let cause = || link_error.map_or(Error::RtClosed, |cause| Error::Network(cause.clone()));
        if let Some(msg) = self.held_exclusive.take() {
            msg.response_tx.send(Err(cause()));
        }
        for entry in self.pending.drain(..) {
            entry.response_tx.send(Err(cause()));
        }
        while let Ok(msg) = self.cmd_rx.try_recv() {
            msg.response_tx.send(Err(cause()));
        }
    }
}

#[cfg(test)]
mod epoch_tests {
    use super::*;

    #[test]
    fn the_epoch_trails_the_latest_msg_id_across_the_wrap() {
        let mut epoch = 0u16;
        for latest in (1..=0x3_0000u32).map(|n| (n & 0xFFFF) as u16) {
            epoch = trailing_epoch(epoch, latest);
            assert!(not_before(latest, epoch));
            assert!(latest.wrapping_sub(epoch) <= EPOCH_TRAIL);
        }
    }

    #[test]
    fn a_recent_reset_epoch_is_kept() {
        assert_eq!(trailing_epoch(100, 120), 100);
        assert_eq!(trailing_epoch(100, 100 + EPOCH_TRAIL), 100);
        assert_eq!(trailing_epoch(100, 101 + EPOCH_TRAIL), 101);
    }
}

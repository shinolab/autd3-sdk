use std::collections::VecDeque;
use std::sync::Arc;
use std::task::{Wake, Waker};
use std::thread::{Thread, ThreadId};
use std::time::{Duration, Instant};

use autd3_rs_core::BusStats;
use autd3_rs_core::rt::oneshot;

use crate::client::MAX_DEVICES;
use crate::error::{Error, NetworkCause};
use crate::protocol::{Cmd, FRAME_BYTES_MAX, Seq, TxFrame};
use crate::transport::{Bus, BusTiming};
use crate::udp::Reply;

use autd3_cpu_wire::Mode;
use autd3_cpu_wire::payload::SetModePayload;
use zerocopy::FromBytes;

use super::Poll;
use super::queue::{CmdMessage, Connect, Queue, TransportConfig};

const PICKUP_POLL: Duration = Duration::from_micros(200);
const RESET_ACK: u8 = 0xFF;

struct Inflight {
    seq: Seq,
    frame: crate::client::pool::Slot,
    acked: u128,
    sent_at: Instant,
    exclusive: bool,
    response_tx: crate::client::completion::CompletionSender,
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

fn stage_frame(seq: Seq, frame: &crate::client::pool::Slot, bufs: &mut [[u8; FRAME_BYTES_MAX]]) {
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

impl TransportConfig {
    fn mode(self) -> Mode {
        if self.low_latency {
            Mode::LowLatency
        } else {
            Mode::Fifo
        }
    }
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

struct Rounds {
    first: u16,
    sent: u32,
    got: u128,
    deadline: Instant,
}

enum Handshake {
    Reset(Rounds),
    Mode(Rounds),
}

enum Phase {
    Detached,
    Handshake {
        done: oneshot::Sender<Result<(), NetworkCause>>,
        step: Handshake,
    },
    Running,
    Closed,
}

enum RoundsOutcome {
    Confirmed,
    Pending,
    Exhausted,
}

fn handshake_failed<E: core::error::Error + Send + Sync + 'static>(e: E) -> NetworkCause {
    tracing::error!("handshake failed: {e}");
    NetworkCause::new(e)
}

struct Unparker(Thread);

impl Wake for Unparker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

pub(crate) struct Engine<B: Bus> {
    bus: B,
    queue: Arc<Queue>,
    done_tx: Option<oneshot::Sender<Option<NetworkCause>>>,
    phase: Phase,
    outcome: Option<NetworkCause>,
    config: TransportConfig,
    timing: BusTiming,
    stats: BusStats,
    seen: u64,
    thread_waker: Option<(ThreadId, Waker)>,
    thread_waker_key: Option<u64>,

    all_acked: u128,
    bufs: Vec<[u8; FRAME_BYTES_MAX]>,

    next_seq: Seq,
    epoch: u16,
    pending: VecDeque<Inflight>,
    held_exclusive: Option<CmdMessage>,
    resync: ResyncState,
    reset: Option<Rounds>,
    head_since: Instant,
    last_send: Instant,
    heartbeat: Option<HeartbeatWait>,
}

impl<B: Bus> Engine<B> {
    pub(crate) fn new(
        bus: B,
        queue: Arc<Queue>,
        done_tx: oneshot::Sender<Option<NetworkCause>>,
        config: TransportConfig,
    ) -> Self {
        let num_devices = bus.num_devices();
        let all_acked: u128 = if num_devices == MAX_DEVICES {
            u128::MAX
        } else {
            (1u128 << num_devices) - 1
        };
        let now = Instant::now();
        Self {
            timing: bus.timing(),
            stats: bus.stats(),
            epoch: bus.next_msg_id(),
            bufs: vec![[0u8; FRAME_BYTES_MAX]; num_devices],
            bus,
            queue,
            done_tx: Some(done_tx),
            phase: Phase::Detached,
            outcome: None,
            config,
            seen: 0,
            thread_waker: None,
            thread_waker_key: None,
            all_acked,
            next_seq: Seq::ZERO,
            pending: VecDeque::new(),
            held_exclusive: None,
            resync: ResyncState::default(),
            reset: None,
            head_since: now,
            last_send: now,
            heartbeat: None,
        }
    }

    pub(crate) fn bus(&self) -> &B {
        &self.bus
    }

    pub(crate) fn queue(&self) -> &Arc<Queue> {
        &self.queue
    }

    pub(crate) fn seen(&self) -> u64 {
        self.seen
    }

    pub(crate) fn is_closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }

    pub(crate) fn outcome(&self) -> Result<(), Error> {
        self.outcome
            .clone()
            .map_or(Ok(()), |cause| Err(Error::Network(cause)))
    }

    pub(crate) fn poll(&mut self) -> Poll {
        if self.is_closed() {
            return Poll::Closed;
        }
        if let Err(cause) = self.step() {
            self.shutdown(Some(cause));
        }
        if self.is_closed() {
            return Poll::Closed;
        }
        Poll::Next(self.next_deadline())
    }

    pub(crate) fn run(&mut self) -> Result<(), Error> {
        while let Poll::Next(deadline) = self.poll() {
            self.wait(deadline);
        }
        self.outcome()
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        self.shutdown(None);
        self.outcome()
    }

    fn step(&mut self) -> Result<(), NetworkCause> {
        while let Some(reply) = self.try_recv()? {
            self.dispatch_reply(&reply)?;
        }
        let requests = self.queue.requests();
        self.seen = requests.generation;
        if requests.close {
            tracing::debug!("close requested");
            self.shutdown(None);
            return Ok(());
        }
        if let Some(connect) = requests.connect {
            self.connect(connect)?;
        }
        let now = Instant::now();
        match self.phase {
            Phase::Detached => self.check_heartbeat(now),
            Phase::Handshake { .. } => self.advance_handshake(now),
            Phase::Running if self.reset.is_some() => {
                self.advance_reset(now)?;
                self.pickup_after_resync()
            }
            Phase::Running => {
                self.pickup()?;
                self.check_head(Instant::now())?;
                self.pickup_after_resync()?;
                if self.reset.is_none() {
                    self.check_heartbeat(Instant::now())?;
                }
                Ok(())
            }
            Phase::Closed => Ok(()),
        }
    }

    fn try_recv(&mut self) -> Result<Option<Reply>, NetworkCause> {
        self.bus.try_recv().map_err(|e| {
            tracing::error!("bus recv failed: {e}");
            NetworkCause::new(e)
        })
    }

    fn send_bufs(&mut self) -> Result<u16, NetworkCause> {
        let msg_id = self.bus.send(&self.bufs).map_err(|e| {
            tracing::error!("bus send failed: {e}");
            NetworkCause::new(e)
        })?;
        self.last_send = Instant::now();
        self.epoch = trailing_epoch(self.epoch, msg_id);
        Ok(msg_id)
    }

    fn stage_all(&mut self, frame: &TxFrame) {
        for buf in &mut self.bufs {
            frame.write_to(buf);
        }
    }

    fn start_rounds(&mut self, frame: &TxFrame) -> Result<Rounds, NetworkCause> {
        self.stage_all(frame);
        let first = self.bus.next_msg_id();
        self.send_bufs()?;
        Ok(Rounds {
            first,
            sent: 1,
            got: 0,
            deadline: Instant::now() + self.config.ack_timeout,
        })
    }

    fn start_reset(&mut self) -> Result<Rounds, NetworkCause> {
        tracing::trace!("sending reset");
        self.start_rounds(&TxFrame::new(Seq::ZERO, Cmd::Reset))
    }

    fn start_mode(&mut self) -> Result<Rounds, NetworkCause> {
        let mut frame = TxFrame::new(Seq::ZERO, Cmd::SetMode);
        let (p, _) = SetModePayload::mut_from_prefix(&mut frame.payload).unwrap();
        p.mode = self.config.mode().as_u8();
        self.start_rounds(&frame)
    }

    fn advance_rounds(
        &mut self,
        now: Instant,
        rounds: &mut Rounds,
    ) -> Result<RoundsOutcome, NetworkCause> {
        if rounds.got == self.all_acked {
            return Ok(RoundsOutcome::Confirmed);
        }
        if now < rounds.deadline {
            return Ok(RoundsOutcome::Pending);
        }
        if rounds.sent >= self.config.max_resync_rounds.get() {
            return Ok(RoundsOutcome::Exhausted);
        }
        tracing::trace!(round = rounds.sent, "resending");
        self.send_bufs()?;
        rounds.sent += 1;
        rounds.deadline = Instant::now() + self.config.ack_timeout;
        Ok(RoundsOutcome::Pending)
    }

    fn reset_confirmed(&mut self) {
        self.epoch = self.bus.next_msg_id();
        self.heartbeat = None;
    }

    fn connect(&mut self, connect: Connect) -> Result<(), NetworkCause> {
        if !matches!(self.phase, Phase::Detached) {
            return Ok(());
        }
        self.config = connect.config;
        self.pending.reserve(self.config.max_inflight.get());
        tracing::debug!(low_latency = self.config.low_latency, "starting handshake");
        let rounds = self.start_reset()?;
        self.phase = Phase::Handshake {
            done: connect.done,
            step: Handshake::Reset(rounds),
        };
        Ok(())
    }

    fn advance_handshake(&mut self, now: Instant) -> Result<(), NetworkCause> {
        let Phase::Handshake { step, .. } = &mut self.phase else {
            return Ok(());
        };
        let (mut rounds, is_reset) = match std::mem::replace(
            step,
            Handshake::Reset(Rounds {
                first: 0,
                sent: 0,
                got: 0,
                deadline: now,
            }),
        ) {
            Handshake::Reset(rounds) => (rounds, true),
            Handshake::Mode(rounds) => (rounds, false),
        };
        let outcome = self.advance_rounds(now, &mut rounds)?;
        let limit = self.config.max_resync_rounds.get();
        let next = match (outcome, is_reset) {
            (RoundsOutcome::Pending, true) => Handshake::Reset(rounds),
            (RoundsOutcome::Pending, false) => Handshake::Mode(rounds),
            (RoundsOutcome::Exhausted, true) => {
                return Err(handshake_failed(HandshakeError::ResetUnconfirmed {
                    rounds: limit,
                }));
            }
            (RoundsOutcome::Exhausted, false) => {
                return Err(handshake_failed(HandshakeError::ModeUnconfirmed {
                    mode: self.config.mode(),
                    rounds: limit,
                }));
            }
            (RoundsOutcome::Confirmed, true) => {
                self.reset_confirmed();
                Handshake::Mode(self.start_mode()?)
            }
            (RoundsOutcome::Confirmed, false) => {
                tracing::info!(mode = ?self.config.mode(), "frame processing mode established");
                self.next_seq = Seq::new(1);
                let Phase::Handshake { done, .. } =
                    std::mem::replace(&mut self.phase, Phase::Running)
                else {
                    unreachable!("checked above");
                };
                if done.send(Ok(())).is_err() {
                    tracing::debug!("the client stopped waiting for the handshake");
                    self.shutdown(None);
                }
                return Ok(());
            }
        };
        if let Phase::Handshake { step, .. } = &mut self.phase {
            *step = next;
        }
        Ok(())
    }

    fn advance_reset(&mut self, now: Instant) -> Result<(), NetworkCause> {
        let Some(mut rounds) = self.reset.take() else {
            return Ok(());
        };
        match self.advance_rounds(now, &mut rounds)? {
            RoundsOutcome::Pending => {
                self.reset = Some(rounds);
                Ok(())
            }
            RoundsOutcome::Confirmed => {
                self.reset_confirmed();
                self.stats.record_reset();
                self.renumber_pending();
                self.head_since = Instant::now();
                self.retransmit_window()?;
                self.check_heartbeat(Instant::now())
            }
            RoundsOutcome::Exhausted => {
                tracing::warn!(
                    pending = self.pending.len(),
                    "the devices did not acknowledge the reset; failing pending frames with timeout"
                );
                self.fail_pending_timeout();
                self.resync = ResyncState::default();
                self.check_heartbeat(Instant::now())
            }
        }
    }

    fn dispatch_reply(&mut self, reply: &Reply) -> Result<(), NetworkCause> {
        let bit = 1u128 << reply.device;
        match &mut self.phase {
            Phase::Handshake {
                step: Handshake::Reset(rounds),
                ..
            } => {
                if not_before(reply.msg_id, rounds.first) && reply.ack == RESET_ACK {
                    rounds.got |= bit;
                }
            }
            Phase::Handshake {
                step: Handshake::Mode(rounds),
                ..
            } => {
                if !not_before(reply.msg_id, self.epoch) || reply.ack != Seq::ZERO.get() {
                    return Ok(());
                }
                if reply.status != 0 {
                    return Err(handshake_failed(HandshakeError::ModeRejected {
                        device: reply.device,
                        mode: self.config.mode(),
                        status: reply.status,
                    }));
                }
                rounds.got |= bit;
            }
            Phase::Running if self.reset.is_some() => {
                let rounds = self.reset.as_mut().expect("just checked");
                if not_before(reply.msg_id, rounds.first) && reply.ack == RESET_ACK {
                    rounds.got |= bit;
                }
            }
            Phase::Detached | Phase::Running => self.handle_reply(reply),
            Phase::Closed => {}
        }
        Ok(())
    }

    fn window_open(&self) -> bool {
        !self.resync.active
            && self.held_exclusive.is_none()
            && !self.pending.front().is_some_and(|entry| entry.exclusive)
            && self.pending.len() < self.config.max_inflight.get()
    }

    fn pickup(&mut self) -> Result<(), NetworkCause> {
        if self.resync.active {
            return Ok(());
        }
        if self.held_exclusive.is_some() {
            if !self.pending.is_empty() {
                return Ok(());
            }
            let msg = self.held_exclusive.take().expect("just checked is_some");
            self.stage_new(msg)?;
        }
        while self.window_open() {
            let Some(msg) = self.queue.pop_cmd() else {
                break;
            };
            self.accept(msg)?;
        }
        Ok(())
    }

    fn pickup_after_resync(&mut self) -> Result<(), NetworkCause> {
        if self.reset.is_some() || self.resync.active {
            return Ok(());
        }
        self.pickup()
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
        self.reset = Some(self.start_reset()?);
        Ok(())
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
            let msg_id = self.bus.heartbeat().map_err(|e| {
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

    fn next_deadline(&self) -> Instant {
        match &self.phase {
            Phase::Handshake {
                step: Handshake::Reset(rounds) | Handshake::Mode(rounds),
                ..
            } => return rounds.deadline,
            Phase::Running if self.reset.is_some() => {
                return self.reset.as_ref().expect("just checked").deadline;
            }
            _ => {}
        }
        let mut deadline = self.last_send + self.timing.heartbeat;
        if let Some(wait) = &self.heartbeat {
            deadline = deadline.min(wait.deadline);
        }
        if !self.pending.is_empty() {
            deadline = deadline.min(self.head_since + self.config.ack_timeout);
        }
        deadline
    }

    fn expects_replies(&self) -> bool {
        !self.pending.is_empty()
            || self.heartbeat.is_some()
            || self.held_exclusive.is_some()
            || self.resync.active
            || self.reset.is_some()
            || matches!(self.phase, Phase::Handshake { .. })
    }

    fn accepts_commands(&self) -> bool {
        match self.phase {
            Phase::Running => self.reset.is_none() && self.window_open(),
            Phase::Detached => true,
            Phase::Handshake { .. } | Phase::Closed => false,
        }
    }

    fn thread_waker(&mut self) -> Waker {
        let current = std::thread::current();
        match &self.thread_waker {
            Some((id, waker)) if *id == current.id() => waker.clone(),
            _ => {
                let waker = Waker::from(Arc::new(Unparker(current.clone())));
                self.thread_waker = Some((current.id(), waker.clone()));
                waker
            }
        }
    }

    pub(crate) fn wait(&mut self, deadline: Instant) {
        if self.is_closed() {
            return;
        }
        if self.expects_replies() {
            loop {
                let now = Instant::now();
                if now >= deadline || self.queue.changed_since(self.seen) {
                    return;
                }
                let until = if self.accepts_commands() {
                    deadline.min(now + PICKUP_POLL)
                } else {
                    deadline
                };
                match self.bus.wait_readable(until) {
                    Ok(false) => {}
                    Ok(true) | Err(_) => return,
                }
            }
        }
        let waker = self.thread_waker();
        loop {
            if self
                .queue
                .register(self.seen, &waker, &mut self.thread_waker_key)
            {
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            std::thread::park_timeout(deadline - now);
        }
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

    fn close_bus(&mut self) -> Option<NetworkCause> {
        match self.bus.close() {
            Ok(()) => None,
            Err(e) => {
                tracing::error!("bus close failed: {e}");
                Some(NetworkCause::new(e))
            }
        }
    }

    fn shutdown(&mut self, cause: Option<NetworkCause>) {
        if self.is_closed() {
            return;
        }
        tracing::debug!(pending = self.pending.len(), "driver closing");
        let (queued, connect) = self.queue.shut();
        let error = || cause.clone().map_or(Error::DriverClosed, Error::Network);
        if let Some(msg) = self.held_exclusive.take() {
            msg.response_tx.send(Err(error()));
        }
        for entry in self.pending.drain(..) {
            entry.response_tx.send(Err(error()));
        }
        for msg in queued {
            msg.response_tx.send(Err(error()));
        }
        let waiting = match std::mem::replace(&mut self.phase, Phase::Closed) {
            Phase::Handshake { done, .. } => Some(done),
            _ => None,
        };
        for done in waiting.into_iter().chain(connect.map(|c| c.done)) {
            if let Some(cause) = &cause {
                let _ = done.send(Err(cause.clone()));
            }
        }
        let closed = self.close_bus();
        self.outcome = cause.or(closed);
        if let Some(done) = self.done_tx.take() {
            let _ = done.send(self.outcome.clone());
        }
    }
}

impl<B: Bus> Drop for Engine<B> {
    fn drop(&mut self) {
        self.shutdown(None);
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

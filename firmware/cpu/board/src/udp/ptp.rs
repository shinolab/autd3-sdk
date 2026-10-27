#![allow(clippy::cast_possible_truncation)]

use autd3_cpu_fw::net::Mac;
use autd3_cpu_fw::ptp::Config;
use autd3_cpu_fw::ptp::clock::complete;
use autd3_cpu_fw::ptp::message::{
    self, Content, FRAME_CAP, Message, MessageType, Outgoing, PortIdentity, correction_ns,
    port_identity,
};
use autd3_cpu_fw::ptp::servo::Servo;
use autd3_cpu_fw::udp::Role;

use super::{RxMeta, Tx, ethsw, other_port, pulse};

const PATH_DELAY_FRACTION_BITS: u32 = 16;
const NS_PER_SEC: u128 = 1_000_000_000;

fn interval_ns(later: u64, earlier: u64, correction: i64) -> Option<i64> {
    i64::try_from(later)
        .ok()?
        .checked_sub(i64::try_from(earlier).ok()?)?
        .checked_sub(correction_ns(correction))
}

fn offset_ns(minuend: i64, subtrahend: i64) -> Option<i64> {
    minuend
        .checked_sub(subtrahend)
        .filter(|offset| offset.checked_neg().is_some())
}

enum TxTimestamp {
    Pending,
    Stale,
    At(u64),
}

enum MasterState {
    Idle,
    WaitSyncTs { seq: u16, since: u32 },
}

#[derive(PartialEq, Eq, Debug)]
enum Pairing {
    Idle,
    GotSync { seq: u16, t2: u64, correction: i64 },
    GotFollowUp { seq: u16, t1: u64, correction: i64 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DelayState {
    Idle,
    WaitReqTs {
        forward: i64,
        t2: u64,
        since: u32,
    },
    WaitResp {
        forward: i64,
        t2: u64,
        t3: u64,
        since: u32,
    },
}

#[derive(Clone, Copy)]
struct Unlocked {
    counted_ms: u32,
    counted_until_ms: u32,
}

impl Unlocked {
    const fn since(now_ms: u32) -> Self {
        Self {
            counted_ms: 0,
            counted_until_ms: now_ms,
        }
    }

    const fn elapsed_ms(self, now_ms: u32) -> u32 {
        self.counted_ms
            .saturating_add(now_ms.wrapping_sub(self.counted_until_ms))
    }

    const fn advance(&mut self, now_ms: u32) {
        self.counted_ms = self.elapsed_ms(now_ms);
        self.counted_until_ms = now_ms;
    }
}

pub(crate) struct Ptp {
    config: Config,
    role: Option<Role>,
    mac: Mac,
    clock_id: [u8; 8],
    upstream: u8,
    downstream: u8,
    seq: u16,
    last_sync_ms: u32,
    master: MasterState,
    pairing: Pairing,
    delay: DelayState,
    path_delay_acc: Option<i64>,
    servo: Servo,
    locked: bool,
    pulse_pending: bool,
    unlocked: Option<Unlocked>,
    last_sample_ms: u32,
    applied_ppb: Option<i32>,
    drift_residual_ppb: i32,
    tx_after: u64,
    last_sync_seq: Option<u16>,
    last_offset: i64,
    pause_syncs: u16,
    pause_futile: bool,
    frame: [u8; FRAME_CAP],
}

impl Default for Ptp {
    fn default() -> Self {
        Self::new()
    }
}

impl Ptp {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            config: Config::DEFAULT,
            role: None,
            mac: [0; 6],
            clock_id: [0; 8],
            upstream: 0,
            downstream: 1,
            seq: 0,
            last_sync_ms: 0,
            master: MasterState::Idle,
            pairing: Pairing::Idle,
            delay: DelayState::Idle,
            path_delay_acc: None,
            servo: Servo::new(),
            locked: false,
            pulse_pending: false,
            unlocked: None,
            last_sample_ms: 0,
            applied_ppb: None,
            drift_residual_ppb: 0,
            tx_after: 0,
            last_sync_seq: None,
            last_offset: 0,
            pause_syncs: 0,
            pause_futile: false,
            frame: [0; FRAME_CAP],
        }
    }

    pub(crate) fn configure(&mut self, role: Role, mac: Mac, upstream: u8) {
        self.reset();
        self.role = Some(role);
        self.mac = mac;
        self.clock_id = autd3_cpu_fw::udp::eui64(mac);
        self.upstream = upstream;
        self.downstream = other_port(self.upstream);
    }

    pub(crate) fn reset(&mut self) {
        let config = self.config;
        *self = Self::new();
        self.config = config;
        ethsw::set_drift(0);
        self.applied_ppb = Some(0);
    }

    pub(crate) const fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    #[must_use]
    pub(crate) const fn role(&self) -> Option<Role> {
        self.role
    }

    #[must_use]
    pub(crate) const fn is_locked(&self) -> bool {
        self.locked
    }

    #[must_use]
    pub(crate) const fn unlocked_ms(&self, now_ms: u32) -> Option<u32> {
        match self.unlocked {
            Some(unlocked) => Some(unlocked.elapsed_ms(now_ms)),
            None => None,
        }
    }

    #[must_use]
    pub(crate) const fn last_offset(&self) -> i64 {
        self.last_offset
    }

    pub(crate) fn time_set(&mut self) {
        if self.role == Some(Role::Grandmaster) {
            self.locked = true;
            self.master = MasterState::Idle;
        }
    }

    pub(crate) fn arm_pulse(&mut self) {
        self.pulse_pending = !pulse::arm();
    }

    fn stop_pulse(&mut self) {
        self.pulse_pending = false;
        pulse::stop();
    }

    fn retry_pulse(&mut self) {
        if self.locked && self.pulse_pending {
            self.arm_pulse();
        }
    }

    #[must_use]
    pub(crate) fn identity(&self) -> PortIdentity {
        port_identity(self.clock_id)
    }

    fn send(&mut self, tx: &mut Tx, out: &Outgoing, port: u8, timestamp: bool) -> bool {
        let len = message::build(&mut self.frame, self.mac, self.clock_id, out);
        tx.send(&self.frame[..len], port, timestamp)
    }

    fn pause_upstream(&mut self, tx: &mut Tx) {
        let Some(quanta) = self.config.pause_quanta else {
            return;
        };
        let len = message::build_pause(&mut self.frame, self.mac, quanta.get());
        let _ = tx.send(&self.frame[..len], self.upstream, false);
    }

    fn pause_upstream_while_congested(&mut self, tx: &mut Tx) {
        if self.pause_syncs > 0 {
            self.pause_upstream(tx);
        }
    }

    fn arm_tx_timestamp(&mut self) {
        ethsw::clear_tx_timestamps();
        self.tx_after = ethsw::capture().unwrap_or(u64::MAX);
    }

    fn take_tx_timestamp(&self, port: u8) -> TxTimestamp {
        let Some(stamp) = ethsw::take_tx_timestamp(port) else {
            return TxTimestamp::Pending;
        };
        if stamp.overwritten {
            return TxTimestamp::Stale;
        }
        let Some(now) = ethsw::capture() else {
            return TxTimestamp::Stale;
        };
        let t = complete(now, stamp.ns);
        if t >= self.tx_after {
            TxTimestamp::At(t)
        } else {
            TxTimestamp::Stale
        }
    }

    pub(crate) fn tick(&mut self, tx: &mut Tx, now_ms: u32) {
        if let Some(unlocked) = &mut self.unlocked {
            unlocked.advance(now_ms);
        }
        match self.role {
            Some(Role::Grandmaster) => self.tick_master(tx, now_ms),
            Some(Role::Slave) => self.tick_slave(now_ms),
            None => {}
            Some(_) => unreachable!(),
        }
    }

    fn tick_master(&mut self, tx: &mut Tx, now_ms: u32) {
        match self.master {
            MasterState::Idle => {
                if !self.locked
                    || now_ms.wrapping_sub(self.last_sync_ms)
                        < self.config.sync_interval.as_millis() as u32
                {
                    return;
                }
                self.last_sync_ms = now_ms;
                self.retry_pulse();
                self.seq = self.seq.wrapping_add(1);
                let seq = self.seq;
                self.pause_upstream_while_congested(tx);
                self.pause_syncs = self.pause_syncs.saturating_sub(1);
                self.arm_tx_timestamp();
                let sync = Outgoing {
                    content: Content::Sync,
                    seq,
                    timestamp: 0,
                    correction: 0,
                };
                if self.send(tx, &sync, self.downstream, true) {
                    self.master = MasterState::WaitSyncTs { seq, since: now_ms };
                }
            }
            MasterState::WaitSyncTs { seq, since } => {
                match self.take_tx_timestamp(self.downstream) {
                    TxTimestamp::At(t1) => {
                        self.master = MasterState::Idle;
                        self.pause_futile = false;
                        let follow_up = Outgoing {
                            content: Content::FollowUp,
                            seq,
                            timestamp: t1,
                            correction: 0,
                        };
                        self.pause_upstream_while_congested(tx);
                        let _ = self.send(tx, &follow_up, self.downstream, false);
                    }
                    TxTimestamp::Stale => self.master = MasterState::Idle,
                    TxTimestamp::Pending
                        if now_ms.wrapping_sub(since) > self.config.holdover.as_millis() as u32 =>
                    {
                        self.master = MasterState::Idle;
                    }
                    TxTimestamp::Pending
                        if now_ms.wrapping_sub(since)
                            > self.config.pause_retry.as_millis() as u32 =>
                    {
                        self.pause_futile = true;
                        self.pause_syncs = 0;
                    }
                    TxTimestamp::Pending if self.pause_futile => {}
                    TxTimestamp::Pending => {
                        self.pause_syncs = self.config.pause_hold_syncs;
                        self.pause_upstream(tx);
                    }
                }
            }
        }
    }

    fn tick_slave(&mut self, now_ms: u32) {
        self.poll_delay_req_timestamp(now_ms);
        if let DelayState::WaitResp { since, .. } = self.delay
            && now_ms.wrapping_sub(since) > self.config.delay_resp_timeout.as_millis() as u32
        {
            self.delay = DelayState::Idle;
        }
        if self.locked
            && now_ms.wrapping_sub(self.last_sample_ms) > self.config.holdover.as_millis() as u32
        {
            self.locked = false;
            self.unlocked = Some(Unlocked::since(now_ms));
            self.servo.unlock();
            self.stop_pulse();
        }
    }

    fn shortest_period_ppb(&self) -> i32 {
        let sync_interval = self.config.sync_interval.as_nanos().max(1);
        i32::try_from(NS_PER_SEC.div_ceil(sync_interval)).unwrap_or(i32::MAX)
    }

    fn dither_drift(&mut self, ppb: i32) -> i32 {
        let floor = self.shortest_period_ppb();
        if ppb.saturating_abs() >= floor {
            self.drift_residual_ppb = 0;
            return ppb;
        }
        let wanted = self.drift_residual_ppb + ppb;
        let written = if wanted >= 0 { floor } else { -floor };
        self.drift_residual_ppb = wanted - written;
        written
    }

    fn apply_drift(&mut self, ppb: i32) {
        let ppb = self.dither_drift(ppb);
        if self.applied_ppb != Some(ppb) {
            ethsw::set_drift(ppb);
            self.applied_ppb = Some(ppb);
        }
    }

    fn poll_delay_req_timestamp(&mut self, now_ms: u32) {
        let DelayState::WaitReqTs { forward, t2, since } = self.delay else {
            return;
        };
        match self.take_tx_timestamp(self.upstream) {
            TxTimestamp::At(t3) => {
                self.delay = DelayState::WaitResp {
                    forward,
                    t2,
                    t3,
                    since: now_ms,
                };
            }
            TxTimestamp::Stale => self.delay = DelayState::Idle,
            TxTimestamp::Pending
                if now_ms.wrapping_sub(since)
                    > self.config.tx_timestamp_timeout.as_millis() as u32 =>
            {
                self.delay = DelayState::Idle;
            }
            TxTimestamp::Pending => {}
        }
    }

    fn path_delay(&self) -> Option<i64> {
        const HALF_BIT: u32 = PATH_DELAY_FRACTION_BITS - 1;
        self.path_delay_acc
            .map(|acc| (acc >> PATH_DELAY_FRACTION_BITS) + ((acc >> HALF_BIT) & 1))
    }

    fn filtered_path_delay(&self, measured: i64) -> Option<i64> {
        let measured = measured.checked_mul(1 << PATH_DELAY_FRACTION_BITS)?;
        match self.path_delay_acc {
            Some(acc) => {
                acc.checked_add(measured.checked_sub(acc)? >> self.config.path_delay_filter_shift)
            }
            None => Some(measured),
        }
    }

    fn delay_req_due(&self, sync_seq: u16) -> bool {
        sync_seq.wrapping_add(u16::from(self.mac[5])) % self.config.delay_req_syncs == 0
    }

    fn paired(
        &mut self,
        tx: &mut Tx,
        seq: u16,
        t1: u64,
        t2: u64,
        corrections: [i64; 2],
        now_ms: u32,
    ) {
        self.pairing = Pairing::Idle;
        let [sync_correction, follow_up_correction] = corrections;
        let Some(forward) = sync_correction
            .checked_add(follow_up_correction)
            .and_then(|correction| interval_ns(t2, t1, correction))
        else {
            return;
        };
        let measure = match (self.locked, self.path_delay()) {
            (true, Some(path_delay)) => {
                let Some(offset) = offset_ns(forward, path_delay) else {
                    return;
                };
                self.sample(offset, t2, now_ms);
                self.locked && self.delay_req_due(seq)
            }
            _ => true,
        };
        if measure {
            self.request_delay(tx, forward, t2, now_ms);
        }
    }

    fn request_delay(&mut self, tx: &mut Tx, forward: i64, t2: u64, now_ms: u32) {
        self.seq = self.seq.wrapping_add(1);
        self.arm_tx_timestamp();
        let req = Outgoing {
            content: Content::DelayReq,
            seq: self.seq,
            timestamp: 0,
            correction: 0,
        };
        self.delay = if self.send(tx, &req, self.upstream, true) {
            DelayState::WaitReqTs {
                forward,
                t2,
                since: now_ms,
            }
        } else {
            DelayState::Idle
        };
    }

    fn sample(&mut self, offset: i64, t2: u64, now_ms: u32) {
        self.last_sample_ms = now_ms;
        self.last_offset = offset;
        let out = self.servo.sample(offset, t2.cast_signed(), &self.config);
        if let Some(ppb) = out.drift_ppb {
            self.apply_drift(ppb);
        }
        if let Some(step) = out.step_ns {
            self.stop_pulse();
            let _ = ethsw::step_time(step);
            if self.locked {
                self.unlocked = Some(Unlocked::since(now_ms));
            }
            self.locked = false;
            self.pairing = Pairing::Idle;
            self.delay = DelayState::Idle;
            self.path_delay_acc = None;
        } else if out.locked_now {
            self.locked = true;
            self.unlocked = None;
            self.arm_pulse();
        } else {
            self.retry_pulse();
        }
    }

    pub(crate) fn on_message(&mut self, tx: &mut Tx, raw: &[u8], rx: RxMeta, now_ms: u32) {
        let Some(msg) = message::parse(raw) else {
            return;
        };
        match self.role {
            Some(Role::Grandmaster) => self.on_master_message(tx, &msg, rx),
            Some(Role::Slave) => self.on_slave_message(tx, &msg, rx, now_ms),
            None => {}
            Some(_) => unreachable!(),
        }
    }

    fn on_master_message(&mut self, tx: &mut Tx, msg: &Message, rx: RxMeta) {
        if msg.kind != MessageType::DelayReq || !self.locked {
            return;
        }
        let Some(now) = ethsw::capture() else {
            return;
        };
        let resp = Outgoing {
            content: Content::DelayResp {
                requesting: msg.source,
            },
            seq: msg.seq,
            timestamp: complete(now, rx.timestamp_ns),
            correction: msg.correction,
        };
        self.pause_upstream_while_congested(tx);
        let _ = self.send(tx, &resp, rx.port, false);
    }

    fn on_slave_message(&mut self, tx: &mut Tx, msg: &Message, rx: RxMeta, now_ms: u32) {
        match msg.kind {
            MessageType::Sync
                if rx.port == self.upstream && self.last_sync_seq != Some(msg.seq) =>
            {
                self.last_sync_seq = Some(msg.seq);
                let Some(now) = ethsw::capture() else {
                    return;
                };
                let t2 = complete(now, rx.timestamp_ns);
                match self.pairing {
                    Pairing::GotFollowUp {
                        seq,
                        t1,
                        correction,
                    } if seq == msg.seq => {
                        self.paired(tx, seq, t1, t2, [msg.correction, correction], now_ms);
                    }
                    Pairing::GotFollowUp { seq, .. }
                        if seq.wrapping_sub(msg.seq).cast_signed() > 0 => {}
                    _ => {
                        self.pairing = Pairing::GotSync {
                            seq: msg.seq,
                            t2,
                            correction: msg.correction,
                        };
                    }
                }
            }
            MessageType::FollowUp if rx.port == self.upstream => match self.pairing {
                Pairing::GotSync {
                    seq,
                    t2,
                    correction,
                } if seq == msg.seq => {
                    self.paired(
                        tx,
                        seq,
                        msg.timestamp,
                        t2,
                        [correction, msg.correction],
                        now_ms,
                    );
                }
                _ if self.follows_last_sync(msg.seq) => {
                    self.pairing = Pairing::GotFollowUp {
                        seq: msg.seq,
                        t1: msg.timestamp,
                        correction: msg.correction,
                    };
                }
                _ => {}
            },
            MessageType::DelayResp if msg.requesting == Some(self.identity()) => {
                self.poll_delay_req_timestamp(now_ms);
                let DelayState::WaitResp {
                    forward, t2, t3, ..
                } = self.delay
                else {
                    return;
                };
                if msg.seq != self.seq {
                    return;
                }
                self.delay = DelayState::Idle;
                let Some(backward) = interval_ns(msg.timestamp, t3, msg.correction) else {
                    return;
                };
                let Some(path_delay_acc) =
                    self.filtered_path_delay(i64::midpoint(forward, backward))
                else {
                    return;
                };
                let Some(difference) = offset_ns(forward, backward) else {
                    return;
                };
                self.path_delay_acc = Some(path_delay_acc);
                if !self.locked {
                    self.sample(difference / 2, t2, now_ms);
                }
            }
            MessageType::Sync
            | MessageType::DelayReq
            | MessageType::FollowUp
            | MessageType::DelayResp => {}
            _ => unreachable!(),
        }
    }

    fn follows_last_sync(&self, seq: u16) -> bool {
        self.last_sync_seq
            .is_none_or(|last| seq.wrapping_sub(last).cast_signed() > 0)
    }
}

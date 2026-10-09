pub mod message;
pub mod servo;

use crate::net::Mac;
use crate::nic::{Nic, RxMeta, complete};
use autd3_cpu_wire::udp::Role;
use message::{
    FRAME_CAP, Message, MessageType, Outgoing, PortIdentity, correction_ns, port_identity,
};
use servo::Servo;

pub use autd3_cpu_wire::config::PtpConfig as Config;
use autd3_cpu_wire::cpu_params;

#[must_use]
pub const fn default_config() -> Config {
    Config {
        sync_interval: cpu_params::PTP_SYNC_INTERVAL,
        tx_timestamp_timeout: cpu_params::PTP_TX_TIMESTAMP_TIMEOUT,
        delay_resp_timeout: cpu_params::PTP_DELAY_RESP_TIMEOUT,
        holdover: cpu_params::PTP_HOLDOVER,
        lock_samples: cpu_params::PTP_LOCK_SAMPLES,
        step_threshold: cpu_params::PTP_STEP_THRESHOLD,
        lock_threshold: cpu_params::PTP_LOCK_THRESHOLD,
        kp_milli: cpu_params::PTP_KP_MILLI,
        ki_milli: cpu_params::PTP_KI_MILLI,
        max_freq_ppb: cpu_params::PTP_MAX_FREQ_PPB,
        delay_req_syncs: cpu_params::PTP_DELAY_REQ_SYNCS,
        path_delay_filter_shift: cpu_params::PTP_PATH_DELAY_FILTER_SHIFT,
        pause_quanta: Some(cpu_params::PTP_PAUSE_QUANTA),
        pause_hold_syncs: cpu_params::PTP_PAUSE_HOLD_SYNCS,
        pause_retry: cpu_params::PTP_PAUSE_RETRY,
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TxTimestamp {
    Pending,
    Stale,
    At(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MasterState {
    Idle,
    WaitSyncTs { seq: u16, since: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

pub struct Ptp {
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
    pub const fn new() -> Self {
        Self {
            config: default_config(),
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

    pub fn configure<N: Nic>(&mut self, nic: &mut N, role: Role, mac: Mac, upstream: u8) {
        self.reset(nic);
        self.role = Some(role);
        self.mac = mac;
        self.clock_id = autd3_cpu_wire::udp::eui64(mac);
        self.upstream = upstream;
        self.downstream = crate::nic::other_port(self.upstream);
    }

    pub fn reset<N: Nic>(&mut self, nic: &mut N) {
        let config = self.config;
        *self = Self::new();
        self.config = config;
        nic.set_drift(0);
        self.applied_ppb = Some(0);
    }

    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }

    pub const fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    #[must_use]
    pub const fn role(&self) -> Option<Role> {
        self.role
    }

    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    #[must_use]
    pub const fn unlocked_ms(&self, now_ms: u32) -> Option<u32> {
        match self.unlocked {
            Some(unlocked) => Some(unlocked.elapsed_ms(now_ms)),
            None => None,
        }
    }

    #[must_use]
    pub const fn last_offset(&self) -> i64 {
        self.last_offset
    }

    pub fn time_set(&mut self) {
        if self.role == Some(Role::Grandmaster) {
            self.locked = true;
            self.master = MasterState::Idle;
        }
    }

    pub fn arm_pulse<N: Nic>(&mut self, nic: &mut N) {
        self.pulse_pending = !nic.arm_pulse();
    }

    fn stop_pulse<N: Nic>(&mut self, nic: &mut N) {
        self.pulse_pending = false;
        nic.stop_pulse();
    }

    fn retry_pulse<N: Nic>(&mut self, nic: &mut N) {
        if self.locked && self.pulse_pending {
            self.arm_pulse(nic);
        }
    }

    #[must_use]
    pub fn identity(&self) -> PortIdentity {
        port_identity(self.clock_id)
    }

    fn send<N: Nic>(&mut self, nic: &mut N, out: &Outgoing, port: u8, timestamp: bool) -> bool {
        let len = message::build(&mut self.frame, self.mac, self.clock_id, out);
        nic.send(&self.frame[..len], port, timestamp)
    }

    fn pause_upstream<N: Nic>(&mut self, nic: &mut N) {
        let Some(quanta) = self.config.pause_quanta else {
            return;
        };
        let len = message::build_pause(&mut self.frame, self.mac, quanta.get());
        let _ = nic.send(&self.frame[..len], self.upstream, false);
    }

    fn pause_upstream_while_congested<N: Nic>(&mut self, nic: &mut N) {
        if self.pause_syncs > 0 {
            self.pause_upstream(nic);
        }
    }

    fn arm_tx_timestamp<N: Nic>(&mut self, nic: &mut N) {
        nic.clear_tx_timestamps();
        self.tx_after = nic.now().unwrap_or(u64::MAX);
    }

    fn take_tx_timestamp<N: Nic>(&self, nic: &mut N, port: u8) -> TxTimestamp {
        let Some(stamp) = nic.take_tx_timestamp(port) else {
            return TxTimestamp::Pending;
        };
        if stamp.overwritten {
            return TxTimestamp::Stale;
        }
        let Some(now) = nic.now() else {
            return TxTimestamp::Stale;
        };
        let t = complete(now, stamp.ns);
        if t >= self.tx_after {
            TxTimestamp::At(t)
        } else {
            TxTimestamp::Stale
        }
    }

    pub fn tick<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        if let Some(unlocked) = &mut self.unlocked {
            unlocked.advance(now_ms);
        }
        match self.role {
            Some(Role::Grandmaster) => self.tick_master(nic, now_ms),
            Some(Role::Slave) => self.tick_slave(nic, now_ms),
            _ => {}
        }
    }

    fn tick_master<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        match self.master {
            MasterState::Idle => {
                if !self.locked
                    || now_ms.wrapping_sub(self.last_sync_ms)
                        < self.config.sync_interval.as_millis() as u32
                {
                    return;
                }
                self.last_sync_ms = now_ms;
                self.retry_pulse(nic);
                self.seq = self.seq.wrapping_add(1);
                let seq = self.seq;
                self.pause_upstream_while_congested(nic);
                self.pause_syncs = self.pause_syncs.saturating_sub(1);
                self.arm_tx_timestamp(nic);
                let sync = Outgoing {
                    kind: MessageType::Sync,
                    seq,
                    timestamp: 0,
                    correction: 0,
                    requesting: None,
                };
                if self.send(nic, &sync, self.downstream, true) {
                    self.master = MasterState::WaitSyncTs { seq, since: now_ms };
                }
            }
            MasterState::WaitSyncTs { seq, since } => {
                match self.take_tx_timestamp(nic, self.downstream) {
                    TxTimestamp::At(t1) => {
                        self.master = MasterState::Idle;
                        self.pause_futile = false;
                        let follow_up = Outgoing {
                            kind: MessageType::FollowUp,
                            seq,
                            timestamp: t1,
                            correction: 0,
                            requesting: None,
                        };
                        self.pause_upstream_while_congested(nic);
                        let _ = self.send(nic, &follow_up, self.downstream, false);
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
                        self.pause_upstream(nic);
                    }
                }
            }
        }
    }

    fn tick_slave<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        self.poll_delay_req_timestamp(nic, now_ms);
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
            self.stop_pulse(nic);
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

    fn apply_drift<N: Nic>(&mut self, nic: &mut N, ppb: i32) {
        let ppb = self.dither_drift(ppb);
        if self.applied_ppb != Some(ppb) {
            nic.set_drift(ppb);
            self.applied_ppb = Some(ppb);
        }
    }

    fn poll_delay_req_timestamp<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        let DelayState::WaitReqTs { forward, t2, since } = self.delay else {
            return;
        };
        match self.take_tx_timestamp(nic, self.upstream) {
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

    fn paired<N: Nic>(
        &mut self,
        nic: &mut N,
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
                self.sample(nic, offset, t2, now_ms);
                self.locked && self.delay_req_due(seq)
            }
            _ => true,
        };
        if measure {
            self.request_delay(nic, forward, t2, now_ms);
        }
    }

    fn request_delay<N: Nic>(&mut self, nic: &mut N, forward: i64, t2: u64, now_ms: u32) {
        self.seq = self.seq.wrapping_add(1);
        self.arm_tx_timestamp(nic);
        let req = Outgoing {
            kind: MessageType::DelayReq,
            seq: self.seq,
            timestamp: 0,
            correction: 0,
            requesting: None,
        };
        self.delay = if self.send(nic, &req, self.upstream, true) {
            DelayState::WaitReqTs {
                forward,
                t2,
                since: now_ms,
            }
        } else {
            DelayState::Idle
        };
    }

    fn sample<N: Nic>(&mut self, nic: &mut N, offset: i64, t2: u64, now_ms: u32) {
        self.last_sample_ms = now_ms;
        self.last_offset = offset;
        let out = self.servo.sample(offset, t2.cast_signed(), &self.config);
        if let Some(ppb) = out.drift_ppb {
            self.apply_drift(nic, ppb);
        }
        if let Some(step) = out.step_ns {
            self.stop_pulse(nic);
            let _ = nic.step(step);
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
            self.arm_pulse(nic);
        } else {
            self.retry_pulse(nic);
        }
    }

    pub fn on_message<N: Nic>(&mut self, nic: &mut N, raw: &[u8], rx: RxMeta, now_ms: u32) {
        let Some(msg) = message::parse(raw) else {
            return;
        };
        match self.role {
            Some(Role::Grandmaster) => self.on_master_message(nic, &msg, rx),
            Some(Role::Slave) => self.on_slave_message(nic, &msg, rx, now_ms),
            _ => {}
        }
    }

    fn on_master_message<N: Nic>(&mut self, nic: &mut N, msg: &Message, rx: RxMeta) {
        if msg.kind != MessageType::DelayReq || !self.locked {
            return;
        }
        let Some(now) = nic.now() else {
            return;
        };
        let resp = Outgoing {
            kind: MessageType::DelayResp,
            seq: msg.seq,
            timestamp: complete(now, rx.timestamp_ns),
            correction: msg.correction,
            requesting: Some(msg.source),
        };
        self.pause_upstream_while_congested(nic);
        let _ = self.send(nic, &resp, rx.port, false);
    }

    fn on_slave_message<N: Nic>(&mut self, nic: &mut N, msg: &Message, rx: RxMeta, now_ms: u32) {
        match msg.kind {
            MessageType::Sync
                if rx.port == self.upstream && self.last_sync_seq != Some(msg.seq) =>
            {
                self.last_sync_seq = Some(msg.seq);
                let Some(now) = nic.now() else {
                    return;
                };
                let t2 = complete(now, rx.timestamp_ns);
                match self.pairing {
                    Pairing::GotFollowUp {
                        seq,
                        t1,
                        correction,
                    } if seq == msg.seq => {
                        self.paired(nic, seq, t1, t2, [msg.correction, correction], now_ms);
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
                        nic,
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
                self.poll_delay_req_timestamp(nic, now_ms);
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
                    self.sample(nic, difference / 2, t2, now_ms);
                }
            }
            _ => {}
        }
    }

    fn follows_last_sync(&self, seq: u16) -> bool {
        self.last_sync_seq
            .is_none_or(|last| seq.wrapping_sub(last).cast_signed() > 0)
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::message::{self, FRAME_CAP, MessageType, Outgoing};
    use autd3_cpu_wire::udp::Role;

    use super::servo::Servo;
    use super::{Config, DelayState, Pairing, Ptp};
    use crate::net::{ETH_HEADER, Mac};
    use crate::nic::{NS_PER_SEC, RxMeta};
    use crate::sim_nic::{Sent, SimClock, SimNic};
    use rstest::rstest;

    const MASTER_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x00];
    const SLAVE_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x01];
    const WIRE_DELAY_NS: f64 = 700.0;
    const HOP_NS: f64 = 10_000.0;
    const TIMER_PERIOD_NS: u64 = 10;

    struct Pair {
        master: Ptp,
        slave: Ptp,
        m: SimNic,
        s: SimNic,
        now_ms: u32,
        delay_reqs: u32,
        drop_delay_reqs: bool,
        pauses: u32,
        rx_jitter_ns: u32,
        rx_jitter_state: u32,
    }

    impl Pair {
        fn new(slave_offset_ns: f64, slave_crystal_ppb: f64) -> Self {
            let start = 1_000_000_000_000.0;
            let mut m = SimNic::with_clock(SimClock::new(start, 0.0));
            let mut s =
                SimNic::with_clock(SimClock::new(start + slave_offset_ns, slave_crystal_ppb));
            let mut master = Ptp::new();
            let mut slave = Ptp::new();
            master.configure(&mut m, Role::Grandmaster, MASTER_MAC, 0);
            slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
            master.time_set();
            Self {
                master,
                slave,
                m,
                s,
                now_ms: 0,
                delay_reqs: 0,
                drop_delay_reqs: false,
                pauses: 0,
                rx_jitter_ns: 0,
                rx_jitter_state: 1,
            }
        }

        fn slave_rx_timestamp(&mut self, at: f64) -> u32 {
            let exact = self.s.local_at(at + WIRE_DELAY_NS);
            if self.rx_jitter_ns == 0 {
                return (exact % NS_PER_SEC) as u32;
            }
            self.rx_jitter_state = self
                .rx_jitter_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let jitter = u64::from((self.rx_jitter_state >> 8) % (2 * self.rx_jitter_ns + 1));
            let jittered = exact + jitter - u64::from(self.rx_jitter_ns);
            ((jittered - jittered % TIMER_PERIOD_NS) % NS_PER_SEC) as u32
        }

        fn deliver(&mut self) {
            loop {
                let from_master: Vec<Sent> = core::mem::take(&mut self.m.sent);
                let from_slave: Vec<Sent> = core::mem::take(&mut self.s.sent);
                if from_master.is_empty() && from_slave.is_empty() {
                    return;
                }
                self.m.t += HOP_NS;
                self.s.t = self.m.t;
                for f in from_master {
                    if f.port != 1 {
                        self.pauses += 1;
                        continue;
                    }
                    let rx = RxMeta {
                        port: 0,
                        timestamp_ns: self.slave_rx_timestamp(f.at),
                    };
                    let now_ms = self.now_ms;
                    self.slave
                        .on_message(&mut self.s, &f.frame[ETH_HEADER..], rx, now_ms);
                }
                for f in from_slave {
                    if f.port != 0 {
                        continue;
                    }
                    if f.frame[ETH_HEADER] & 0x0F == MessageType::DelayReq.as_u8() {
                        self.delay_reqs += 1;
                        if self.drop_delay_reqs {
                            continue;
                        }
                    }
                    let rx = RxMeta {
                        port: 1,
                        timestamp_ns: (self.m.local_at(f.at + WIRE_DELAY_NS) % NS_PER_SEC) as u32,
                    };
                    let now_ms = self.now_ms;
                    self.master
                        .on_message(&mut self.m, &f.frame[ETH_HEADER..], rx, now_ms);
                }
            }
        }

        fn run_ms(&mut self, ms: u32) {
            for _ in 0..ms {
                self.m.t += 1e6;
                self.s.t = self.m.t;
                self.now_ms += 1;
                let now_ms = self.now_ms;
                self.master.tick(&mut self.m, now_ms);
                self.slave.tick(&mut self.s, now_ms);
                self.deliver();
            }
        }

        fn true_offset(&self) -> f64 {
            (self.s.clock.at(self.s.t) - self.m.clock.at(self.m.t)) as f64
        }
    }

    #[test]
    fn a_slave_steps_once_and_then_locks() {
        let mut pair = Pair::new(3_000_000.0, 40_000.0);
        pair.run_ms(4000);
        assert_eq!(pair.s.steps, 1);
        assert_eq!(pair.s.steps_with_pulse, 0);
        assert_eq!(pair.s.pulse_armed, 1);
        assert!(pair.slave.is_locked());
        assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
        assert!(pair.slave.last_offset().abs() < 100);
    }

    #[test]
    fn a_slave_whose_pulse_fails_to_arm_arms_it_again_on_the_following_samples() {
        let mut pair = Pair::new(2_000.0, -15_000.0);
        pair.s.pulse_arm_failures = 2;
        for _ in 0..4000 {
            pair.run_ms(1);
            if pair.slave.is_locked() {
                break;
            }
        }
        assert!(pair.slave.is_locked());
        assert_eq!(pair.s.pulse_arm_failed, 1);
        assert!(!pair.s.pulse_running);
        pair.run_ms(1000);
        assert!(pair.slave.is_locked());
        assert_eq!(pair.s.pulse_arm_failed, 2);
        assert_eq!(pair.s.pulse_armed, 1);
        assert!(pair.s.pulse_running);
        pair.run_ms(1000);
        assert_eq!(pair.s.pulse_armed, 1);
    }

    #[test]
    fn a_grandmaster_whose_pulse_fails_to_arm_arms_it_again_when_it_sends_sync() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.m.pulse_arm_failures = 2;
        pair.master.arm_pulse(&mut pair.m);
        assert_eq!(pair.m.pulse_arm_failed, 1);
        assert!(!pair.m.pulse_running);
        pair.run_ms(1000);
        assert_eq!(pair.m.pulse_arm_failed, 2);
        assert_eq!(pair.m.pulse_armed, 1);
        assert!(pair.m.pulse_running);
        pair.run_ms(1000);
        assert_eq!(pair.m.pulse_armed, 1);
    }

    #[test]
    fn a_pulse_that_failed_to_arm_is_not_armed_again_after_the_lock_is_lost() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.s.pulse_arm_failures = u32::MAX;
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        assert!(pair.slave.pulse_pending);
        let last_sample = pair.slave.last_sample_ms;
        let lost = last_sample + Config::default().holdover.as_millis() as u32 + 1;
        pair.slave.tick(&mut pair.s, lost);
        assert!(!pair.slave.is_locked());
        assert!(!pair.slave.pulse_pending);
    }

    #[test]
    fn a_small_initial_offset_locks_without_a_step() {
        let mut pair = Pair::new(2_000.0, -15_000.0);
        pair.run_ms(4000);
        assert_eq!(pair.s.steps, 0);
        assert_eq!(pair.s.pulse_armed, 1);
        assert_eq!(pair.s.pulse_stopped, 0);
        assert!(pair.slave.is_locked());
        assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
    }

    #[test]
    fn the_lock_holds_over_a_long_run() {
        let mut pair = Pair::new(-7_000_000.0, 93_000.0);
        pair.run_ms(3000);
        assert!(pair.slave.is_locked());
        let steps = pair.s.steps;
        for _ in 0..20 {
            pair.run_ms(500);
            assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
        }
        assert_eq!(pair.s.steps, steps);
    }

    #[test]
    fn crystals_at_plus_and_minus_100_ppm_stay_within_100_ns() {
        for crystal_ppb in [100_000.0, -100_000.0] {
            let mut pair = Pair::new(50_000_000.0, crystal_ppb);
            pair.run_ms(4000);
            assert!(pair.slave.is_locked(), "{crystal_ppb}");
            let steps = pair.s.steps;
            for _ in 0..40 {
                pair.run_ms(250);
                assert!(
                    pair.true_offset().abs() < 100.0,
                    "{crystal_ppb}: {}",
                    pair.true_offset()
                );
            }
            assert_eq!(pair.s.steps, steps);
        }
    }

    #[test]
    fn a_crystal_close_to_the_grandmaster_stays_within_60_ns() {
        for rx_jitter_ns in [0, 5, 20] {
            for crystal_ppb in [
                0.0, 1.0, -2.0, 5.0, -10.0, 20.0, -30.0, 45.0, -62.0, 80.0, -120.0, 170.0, -250.0,
                300.0, -450.0,
            ] {
                let mut pair = Pair::new(2_000.0, crystal_ppb);
                pair.rx_jitter_ns = rx_jitter_ns;
                pair.run_ms(5000);
                assert!(pair.slave.is_locked(), "{rx_jitter_ns} {crystal_ppb}");
                for _ in 0..600 {
                    pair.run_ms(100);
                    assert!(
                        pair.true_offset().abs() < 60.0,
                        "{rx_jitter_ns} {crystal_ppb}: {}",
                        pair.true_offset()
                    );
                }
                assert_eq!(pair.s.steps, 0, "{rx_jitter_ns} {crystal_ppb}");
            }
        }
    }

    #[test]
    fn a_master_is_silent_until_its_time_is_set() {
        let mut m = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut master = Ptp::new();
        master.configure(&mut m, Role::Grandmaster, MASTER_MAC, 0);
        for ms in 0..100 {
            master.tick(&mut m, ms);
        }
        assert!(m.sent.is_empty());
        master.time_set();
        master.tick(&mut m, 100);
        assert_eq!(m.sent.len(), 1);
        assert_eq!(m.sent[0].port, 1);
        assert_eq!(
            m.sent[0].frame[ETH_HEADER] & 0x0F,
            MessageType::Sync.as_u8()
        );
    }

    #[test]
    fn a_master_holds_the_next_sync_until_the_previous_one_is_stamped() {
        use crate::nic::Nic;

        let mut m = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut master = Ptp::new();
        master.configure(&mut m, Role::Grandmaster, MASTER_MAC, 0);
        master.time_set();
        master.tick(&mut m, 100);
        assert_eq!(m.sent.len(), 1);
        m.clear_tx_timestamps();
        let holdover = Config::default().holdover.as_millis() as u32;
        for ms in 1..=holdover {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
        }
        assert_eq!(downstream(&m).len(), 1);

        let egress = (m.local() % NS_PER_SEC) as u32;
        m.stamp_tx(1, egress);
        master.tick(&mut m, 100 + holdover);
        let sent = downstream(&m);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1][ETH_HEADER] & 0x0F, MessageType::FollowUp.as_u8());
        let follow_up = message::parse(&sent[1][ETH_HEADER..]).unwrap();
        assert_eq!(follow_up.timestamp % NS_PER_SEC, u64::from(egress));
        master.tick(&mut m, 101 + holdover);
        let sent = downstream(&m);
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[2][ETH_HEADER] & 0x0F, MessageType::Sync.as_u8());
    }

    fn downstream(nic: &SimNic) -> Vec<Vec<u8>> {
        nic.sent
            .iter()
            .filter(|f| f.port == 1)
            .map(|f| f.frame.clone())
            .collect()
    }

    fn pauses(nic: &SimNic) -> usize {
        nic.sent
            .iter()
            .filter(|f| f.port == 0 && f.frame[12..14] == [0x88, 0x08])
            .count()
    }

    fn stuck_master() -> (Ptp, SimNic) {
        use crate::nic::Nic;

        let mut m = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut master = Ptp::new();
        master.configure(&mut m, Role::Grandmaster, MASTER_MAC, 0);
        master.time_set();
        master.tick(&mut m, 100);
        m.clear_tx_timestamps();
        (master, m)
    }

    #[test]
    fn a_master_pauses_the_host_while_its_sync_is_stuck() {
        let (mut master, mut m) = stuck_master();
        assert_eq!(pauses(&m), 0);
        for ms in 1..=3 {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
        }
        assert_eq!(pauses(&m), 3);

        let egress = (m.local() % NS_PER_SEC) as u32;
        m.stamp_tx(1, egress);
        master.tick(&mut m, 104);
        assert_eq!(pauses(&m), 4);
        assert_eq!(downstream(&m).len(), 2);
        master.tick(&mut m, 116);
        assert_eq!(pauses(&m), 5);
        assert_eq!(downstream(&m).len(), 3);
    }

    #[test]
    fn a_master_whose_pauses_do_not_free_the_sync_stops_pausing() {
        use crate::nic::Nic;

        let (mut master, mut m) = stuck_master();
        for ms in 1..=10_000 {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
            m.clear_tx_timestamps();
        }
        assert_eq!(pauses(&m), 8);
        assert!(downstream(&m).len() > 5);

        let egress = (m.local() % NS_PER_SEC) as u32;
        m.stamp_tx(1, egress);
        for ms in 10_001..=10_040 {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
            m.clear_tx_timestamps();
        }
        assert!(pauses(&m) > 8);
    }

    #[test]
    fn a_master_without_pause_quanta_never_pauses() {
        let (mut master, mut m) = stuck_master();
        master.set_config(Config {
            pause_quanta: None,
            ..Config::default()
        });
        for ms in 1..=20 {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
        }
        assert_eq!(pauses(&m), 0);
    }

    #[test]
    fn the_pause_retry_and_the_quanta_follow_the_config() {
        let (mut master, mut m) = stuck_master();
        master.set_config(Config {
            pause_quanta: core::num::NonZeroU16::new(0x1234),
            pause_retry: core::time::Duration::from_millis(3),
            ..Config::default()
        });
        for ms in 1..=20 {
            m.t += 1e6;
            master.tick(&mut m, 100 + ms);
        }
        assert_eq!(pauses(&m), 3);
        let pause = m.sent.iter().find(|f| f.port == 0).unwrap();
        assert_eq!(pause.frame[ETH_HEADER..][2..4], [0x12, 0x34]);
    }

    #[test]
    fn the_pause_hold_follows_the_config() {
        let (mut master, mut m) = stuck_master();
        master.set_config(Config {
            pause_hold_syncs: 0,
            ..Config::default()
        });
        m.t += 1e6;
        master.tick(&mut m, 101);
        assert_eq!(pauses(&m), 1);
        let egress = (m.local() % NS_PER_SEC) as u32;
        m.stamp_tx(1, egress);
        master.tick(&mut m, 102);
        master.tick(&mut m, 116);
        assert_eq!(downstream(&m).len(), 3);
        assert_eq!(pauses(&m), 1);
    }

    #[test]
    fn the_delay_request_interval_follows_the_config() {
        let mut pair = Pair::new(2_000.0, 20_000.0);
        pair.slave.set_config(Config {
            delay_req_syncs: core::num::NonZeroU16::new(4).unwrap(),
            ..Config::default()
        });
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        let before = pair.delay_reqs;
        pair.run_ms(8 * 4 * 10);
        assert_eq!(pair.delay_reqs - before, 10);
    }

    #[test]
    fn the_path_delay_filter_follows_the_shift_and_survives_a_change_of_it() {
        let mut s = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        slave.path_delay_acc = slave.filtered_path_delay(1_000);
        assert_eq!(slave.path_delay(), Some(1_000));
        slave.path_delay_acc = slave.filtered_path_delay(1_800);
        assert_eq!(slave.path_delay(), Some(1_025));

        slave.set_config(Config {
            path_delay_filter_shift: 0,
            ..Config::default()
        });
        assert_eq!(slave.path_delay(), Some(1_025));
        slave.path_delay_acc = slave.filtered_path_delay(700);
        assert_eq!(slave.path_delay(), Some(700));

        slave.set_config(Config {
            path_delay_filter_shift: 16,
            ..Config::default()
        });
        slave.path_delay_acc = slave.filtered_path_delay(-700);
        assert_eq!(slave.path_delay(), Some(700));
    }

    #[test]
    fn a_step_forgets_the_filtered_path_delay() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        assert!(pair.slave.path_delay().is_some());
        pair.s.jump(1_000_000);
        for _ in 0..200 {
            pair.run_ms(1);
            if pair.s.steps == 1 {
                break;
            }
        }
        assert_eq!(pair.s.steps, 1);
        assert_eq!(pair.slave.path_delay(), None);
    }

    #[test]
    fn a_master_that_is_not_congested_sends_no_pause() {
        let mut pair = Pair::new(2_000.0, 20_000.0);
        pair.run_ms(1000);
        assert_eq!(pair.m.sent.len(), 0);
        assert_eq!(pair.pauses, 0);
    }

    #[test]
    fn a_master_gives_up_an_unstamped_sync_after_the_holdover() {
        use crate::nic::Nic;

        let mut m = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut master = Ptp::new();
        master.configure(&mut m, Role::Grandmaster, MASTER_MAC, 0);
        master.time_set();
        master.tick(&mut m, 100);
        m.clear_tx_timestamps();
        let holdover = Config::default().holdover.as_millis() as u32;
        master.tick(&mut m, 100 + holdover);
        assert_eq!(downstream(&m).len(), 1);
        master.tick(&mut m, 101 + holdover);
        master.tick(&mut m, 102 + holdover);
        let sent = downstream(&m);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1][ETH_HEADER] & 0x0F, MessageType::Sync.as_u8());
    }

    #[test]
    fn a_sync_from_the_downstream_port_is_ignored() {
        let mut s = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        let mut buf = [0u8; FRAME_CAP];
        let sync = Outgoing {
            kind: MessageType::Sync,
            seq: 1,
            timestamp: 0,
            correction: 0,
            requesting: None,
        };
        let len = message::build(&mut buf, MASTER_MAC, [0; 8], &sync);
        let follow_up = Outgoing {
            kind: MessageType::FollowUp,
            seq: 1,
            timestamp: 5,
            correction: 0,
            requesting: None,
        };
        let mut fu = [0u8; FRAME_CAP];
        let fu_len = message::build(&mut fu, MASTER_MAC, [0; 8], &follow_up);
        let from_downstream = RxMeta {
            port: 1,
            timestamp_ns: 10,
        };
        slave.on_message(&mut s, &buf[ETH_HEADER..len], from_downstream, 0);
        slave.on_message(&mut s, &fu[ETH_HEADER..fu_len], from_downstream, 0);
        assert!(s.sent.is_empty());
        let from_upstream = RxMeta {
            port: 0,
            timestamp_ns: 10,
        };
        slave.on_message(&mut s, &buf[ETH_HEADER..len], from_upstream, 0);
        slave.on_message(&mut s, &fu[ETH_HEADER..fu_len], from_upstream, 0);
        assert_eq!(s.sent.len(), 1);
        assert_eq!(s.sent[0].port, 0);
    }

    fn frame(kind: MessageType, seq: u16, timestamp: u64) -> ([u8; FRAME_CAP], usize) {
        let mut buf = [0u8; FRAME_CAP];
        let out = Outgoing {
            kind,
            seq,
            timestamp,
            correction: 0,
            requesting: None,
        };
        let len = message::build(&mut buf, MASTER_MAC, [0; 8], &out);
        (buf, len)
    }

    #[test]
    fn a_follow_up_that_arrives_before_its_sync_still_pairs() {
        let mut s = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        let from_upstream = RxMeta {
            port: 0,
            timestamp_ns: 10,
        };
        let (fu, fu_len) = frame(MessageType::FollowUp, 1, 5);
        let (sync, sync_len) = frame(MessageType::Sync, 1, 0);
        slave.on_message(&mut s, &fu[ETH_HEADER..fu_len], from_upstream, 0);
        assert!(s.sent.is_empty());
        slave.on_message(&mut s, &sync[ETH_HEADER..sync_len], from_upstream, 0);
        assert_eq!(s.sent.len(), 1);
        assert_eq!(
            s.sent[0].frame[ETH_HEADER] & 0x0F,
            MessageType::DelayReq.as_u8()
        );
        assert_eq!(
            slave.delay,
            DelayState::WaitReqTs {
                forward: 5,
                t2: 10,
                since: 0
            }
        );
    }

    #[test]
    fn a_follow_up_of_an_already_seen_sync_does_not_displace_the_next_pair() {
        let mut s = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        let from_upstream = RxMeta {
            port: 0,
            timestamp_ns: 10,
        };
        let (sync, sync_len) = frame(MessageType::Sync, 2, 0);
        let (stale, stale_len) = frame(MessageType::FollowUp, 1, 5);
        let (fu, fu_len) = frame(MessageType::FollowUp, 2, 5);
        slave.on_message(&mut s, &sync[ETH_HEADER..sync_len], from_upstream, 0);
        slave.on_message(&mut s, &stale[ETH_HEADER..stale_len], from_upstream, 0);
        assert!(s.sent.is_empty());
        slave.on_message(&mut s, &fu[ETH_HEADER..fu_len], from_upstream, 0);
        assert_eq!(s.sent.len(), 1);
    }

    #[test]
    fn a_locked_slave_asks_for_the_delay_once_in_thirty_two_syncs() {
        let mut pair = Pair::new(2_000.0, 20_000.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        let before = pair.delay_reqs;
        pair.run_ms(8 * 32 * 10);
        assert_eq!(pair.delay_reqs - before, 10);
        assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
    }

    #[test]
    fn slaves_ask_for_the_delay_on_different_syncs() {
        let mut nic = SimNic::with_clock(SimClock::new(0.0, 0.0));
        let mut due = |id: u8| {
            let mut slave = Ptp::new();
            slave.configure(&mut nic, Role::Slave, autd3_cpu_wire::udp::mac(id), 0);
            (0..32u16)
                .filter(|&seq| slave.delay_req_due(seq))
                .collect::<Vec<_>>()
        };
        assert_eq!(due(1), [31]);
        assert_eq!(due(2), [30]);
        assert_eq!(due(32), [0]);
    }

    #[test]
    fn a_locked_slave_keeps_its_lock_without_delay_resp() {
        let mut pair = Pair::new(2_000.0, 20_000.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        pair.drop_delay_reqs = true;
        let stopped = pair.s.pulse_stopped;
        for _ in 0..20 {
            pair.run_ms(500);
            assert!(pair.slave.is_locked());
            assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
        }
        assert_eq!(pair.s.pulse_stopped, stopped);
    }

    #[test]
    fn an_unlocked_slave_asks_for_the_delay_on_every_sync() {
        let mut pair = Pair::new(2_000.0, 20_000.0);
        pair.run_ms(8 * 20 + 4);
        assert!(!pair.slave.is_locked());
        assert_eq!(pair.delay_reqs, 20);
    }

    #[test]
    fn a_delay_resp_for_another_slave_is_ignored() {
        let mut pair = Pair::new(0.0, 0.0);
        pair.run_ms(40);
        let waiting = DelayState::WaitResp {
            forward: 5_000,
            t2: 6_000,
            t3: 7_000,
            since: 40,
        };
        pair.slave.delay = waiting;
        let before = pair.slave.last_offset();
        let last_sample = pair.slave.last_sample_ms;
        let rx = RxMeta {
            port: 0,
            timestamp_ns: 0,
        };
        let mut buf = [0u8; FRAME_CAP];
        let foreign = Outgoing {
            kind: MessageType::DelayResp,
            seq: pair.slave.seq,
            timestamp: 2_000,
            correction: 0,
            requesting: Some([9; 10]),
        };
        let len = message::build(&mut buf, MASTER_MAC, [0; 8], &foreign);
        pair.slave
            .on_message(&mut pair.s, &buf[ETH_HEADER..len], rx, 41);
        assert_eq!(pair.slave.delay, waiting);
        assert_eq!(pair.slave.last_offset(), before);
        assert_eq!(pair.slave.last_sample_ms, last_sample);

        let own = Outgoing {
            requesting: Some(pair.slave.identity()),
            ..foreign
        };
        let len = message::build(&mut buf, MASTER_MAC, [0; 8], &own);
        pair.slave
            .on_message(&mut pair.s, &buf[ETH_HEADER..len], rx, 41);
        assert_eq!(pair.slave.delay, DelayState::Idle);
        assert_eq!(pair.slave.last_offset(), 5_000);
        assert_eq!(pair.slave.last_sample_ms, 41);
    }

    fn frame_with_correction(
        kind: MessageType,
        seq: u16,
        timestamp: u64,
        correction: i64,
        requesting: Option<[u8; 10]>,
    ) -> ([u8; FRAME_CAP], usize) {
        let mut buf = [0u8; FRAME_CAP];
        let out = Outgoing {
            kind,
            seq,
            timestamp,
            correction,
            requesting,
        };
        let len = message::build(&mut buf, MASTER_MAC, [0; 8], &out);
        (buf, len)
    }

    fn deliver_sync_pair(
        slave: &mut Ptp,
        nic: &mut SimNic,
        seq: u16,
        t1: u64,
        corrections: [i64; 2],
        follow_up_first: bool,
        now_ms: u32,
    ) {
        let rx = RxMeta {
            port: 0,
            timestamp_ns: (nic.local() % NS_PER_SEC) as u32,
        };
        let (sync, sync_len) =
            frame_with_correction(MessageType::Sync, seq, 0, corrections[0], None);
        let (fu, fu_len) =
            frame_with_correction(MessageType::FollowUp, seq, t1, corrections[1], None);
        let sync = &sync[ETH_HEADER..sync_len];
        let fu = &fu[ETH_HEADER..fu_len];
        if follow_up_first {
            slave.on_message(nic, fu, rx, now_ms);
            slave.on_message(nic, sync, rx, now_ms);
        } else {
            slave.on_message(nic, sync, rx, now_ms);
            slave.on_message(nic, fu, rx, now_ms);
        }
    }

    #[rstest]
    #[case::t1_beyond_the_signed_range(u64::MAX, [0, 0])]
    #[case::t1_and_correction_overflow_the_difference(i64::MAX.cast_unsigned(), [0, i64::MAX])]
    #[case::corrections_overflow_their_sum(5, [i64::MAX, i64::MAX])]
    fn an_unlocked_slave_discards_a_sync_pair_that_is_out_of_range(
        #[case] t1: u64,
        #[case] corrections: [i64; 2],
        #[values(false, true)] follow_up_first: bool,
    ) {
        let mut s = SimNic::with_clock(SimClock::new(10.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        deliver_sync_pair(&mut slave, &mut s, 1, t1, corrections, follow_up_first, 0);
        assert!(s.sent.is_empty());
        assert_eq!(slave.delay, DelayState::Idle);
        assert_eq!(slave.pairing, Pairing::Idle);
        assert_eq!(slave.servo, Servo::new());
        assert_eq!(s.steps, 0);
    }

    #[rstest]
    #[case::t1_beyond_the_signed_range(u64::MAX, [0, 0])]
    #[case::t1_and_correction_overflow_the_difference(i64::MAX.cast_unsigned(), [0, i64::MAX])]
    #[case::corrections_overflow_their_sum(5, [i64::MAX, i64::MAX])]
    fn a_locked_slave_discards_a_sync_pair_that_is_out_of_range(
        #[case] t1: u64,
        #[case] corrections: [i64; 2],
        #[values(false, true)] follow_up_first: bool,
    ) {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        let seq = pair.slave.last_sync_seq.unwrap().wrapping_add(1);
        let steps = pair.s.steps;
        let last_offset = pair.slave.last_offset();
        let last_sample = pair.slave.last_sample_ms;
        let servo = pair.slave.servo;
        let delay = pair.slave.delay;
        let now_ms = pair.now_ms;
        deliver_sync_pair(
            &mut pair.slave,
            &mut pair.s,
            seq,
            t1,
            corrections,
            follow_up_first,
            now_ms,
        );
        assert!(pair.slave.is_locked());
        assert_eq!(pair.s.steps, steps);
        assert_eq!(pair.slave.last_offset(), last_offset);
        assert_eq!(pair.slave.last_sample_ms, last_sample);
        assert_eq!(pair.slave.servo, servo);
        assert_eq!(pair.slave.delay, delay);
        assert_eq!(pair.slave.pairing, Pairing::Idle);
        assert!(pair.s.sent.is_empty());
    }

    #[test]
    fn a_locked_slave_discards_an_offset_that_is_out_of_range() {
        let mut s = SimNic::with_clock(SimClock::new(10.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        slave.locked = true;
        slave.path_delay_acc = Some(i64::MAX);
        deliver_sync_pair(
            &mut slave,
            &mut s,
            1,
            i64::MAX.cast_unsigned(),
            [0, 0],
            false,
            7,
        );
        assert!(s.sent.is_empty());
        assert_eq!(slave.last_sample_ms, 0);
        assert_eq!(slave.last_offset(), 0);
        assert_eq!(slave.servo, Servo::new());
        assert_eq!(slave.delay, DelayState::Idle);
        assert_eq!(s.steps, 0);
    }

    #[rstest]
    #[case::t4_beyond_the_signed_range(5_000, u64::MAX, 0)]
    #[case::t4_and_correction_overflow_the_difference(5_000, i64::MAX.cast_unsigned(), i64::MIN)]
    #[case::forward_and_backward_overflow_the_offset(-i64::MAX, i64::MAX.cast_unsigned(), 0)]
    #[case::path_delay_beyond_the_filter_range(5_000, 1 << 50, 0)]
    fn a_slave_discards_a_delay_resp_that_is_out_of_range(
        #[case] forward: i64,
        #[case] t4: u64,
        #[case] correction: i64,
    ) {
        let mut s = SimNic::with_clock(SimClock::new(10.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        slave.delay = DelayState::WaitResp {
            forward,
            t2: 6_000,
            t3: 7_000,
            since: 0,
        };
        let (resp, len) = frame_with_correction(
            MessageType::DelayResp,
            slave.seq,
            t4,
            correction,
            Some(slave.identity()),
        );
        let rx = RxMeta {
            port: 0,
            timestamp_ns: 0,
        };
        slave.on_message(&mut s, &resp[ETH_HEADER..len], rx, 9);
        assert_eq!(slave.delay, DelayState::Idle);
        assert_eq!(slave.path_delay(), None);
        assert_eq!(slave.last_sample_ms, 0);
        assert_eq!(slave.last_offset(), 0);
        assert_eq!(slave.servo, Servo::new());
        assert_eq!(s.steps, 0);
    }

    #[test]
    fn a_timestamp_beyond_the_nanosecond_range_is_not_paired() {
        let mut s = SimNic::with_clock(SimClock::new(10.0, 0.0));
        let mut slave = Ptp::new();
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        let rx = RxMeta {
            port: 0,
            timestamp_ns: 10,
        };
        let (sync, sync_len) = frame(MessageType::Sync, 1, 0);
        let (mut fu, fu_len) = frame(MessageType::FollowUp, 1, 5);
        fu[ETH_HEADER + 34..][..6].fill(0xFF);
        slave.on_message(&mut s, &sync[ETH_HEADER..sync_len], rx, 0);
        slave.on_message(&mut s, &fu[ETH_HEADER..fu_len], rx, 0);
        assert!(s.sent.is_empty());
        assert_eq!(slave.delay, DelayState::Idle);
        assert_eq!(slave.servo, Servo::new());
    }

    #[test]
    fn a_locked_slave_that_sees_a_jump_stops_its_pulse_steps_and_locks_again() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        assert_eq!(pair.s.pulse_armed, 1);
        let stopped = pair.s.pulse_stopped;
        pair.s.jump(1_000_000);
        for _ in 0..200 {
            pair.run_ms(1);
            if pair.s.steps == 1 {
                break;
            }
        }
        assert_eq!(pair.s.steps, 1);
        assert_eq!(pair.s.steps_with_pulse, 0);
        assert_eq!(pair.s.pulse_stopped, stopped + 1);
        assert_eq!(pair.s.pulse_armed, 1);
        assert!(!pair.slave.is_locked());
        assert!(pair.slave.unlocked.is_some());
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        assert!(pair.slave.unlocked.is_none());
        assert_eq!(pair.s.steps_with_pulse, 0);
        assert_eq!(pair.s.pulse_stopped, stopped + pair.s.steps);
        assert_eq!(pair.s.pulse_armed, 2);
    }

    #[test]
    fn a_step_before_the_first_lock_is_not_a_lost_lock() {
        let mut pair = Pair::new(3_000_000.0, 40_000.0);
        for _ in 0..4000 {
            pair.run_ms(1);
            if pair.s.steps == 1 {
                break;
            }
        }
        assert_eq!(pair.s.steps, 1);
        assert!(!pair.slave.is_locked());
        assert!(pair.slave.unlocked.is_none());
    }

    #[test]
    fn a_locked_slave_stops_its_pulse_when_the_holdover_runs_out() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        let stopped = pair.s.pulse_stopped;
        let last_sample = pair.slave.last_sample_ms;
        for ms in 1..=Config::default().holdover.as_millis() as u32 {
            pair.slave.tick(&mut pair.s, last_sample + ms);
        }
        assert!(pair.slave.is_locked());
        assert_eq!(pair.s.pulse_stopped, stopped);
        let lost = last_sample + Config::default().holdover.as_millis() as u32 + 1;
        assert_eq!(pair.slave.unlocked_ms(lost), None);
        pair.slave.tick(&mut pair.s, lost);
        assert!(!pair.slave.is_locked());
        assert_eq!(pair.slave.unlocked_ms(lost), Some(0));
        assert_eq!(pair.slave.unlocked_ms(lost + 7), Some(7));
        assert_eq!(pair.s.pulse_stopped, stopped + 1);
        pair.slave.tick(
            &mut pair.s,
            last_sample + Config::default().holdover.as_millis() as u32 + 2,
        );
        assert_eq!(pair.s.pulse_stopped, stopped + 1);
        assert_eq!(pair.s.pulse_armed, 1);
    }

    #[test]
    fn the_unlocked_time_saturates_past_the_millisecond_counter_wrap() {
        let mut pair = Pair::new(2_000.0, 0.0);
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        let lost = pair.slave.last_sample_ms + Config::default().holdover.as_millis() as u32 + 1;
        pair.slave.tick(&mut pair.s, lost);
        assert!(!pair.slave.is_locked());
        assert_eq!(pair.slave.unlocked_ms(lost), Some(0));

        let half = lost.wrapping_add(1 << 31);
        pair.slave.tick(&mut pair.s, half);
        assert_eq!(pair.slave.unlocked_ms(half), Some(1 << 31));

        let before_wrap = lost.wrapping_add(u32::MAX);
        pair.slave.tick(&mut pair.s, before_wrap);
        assert_eq!(pair.slave.unlocked_ms(before_wrap), Some(u32::MAX));

        let wrapped = lost.wrapping_add(5);
        assert_eq!(pair.slave.unlocked_ms(wrapped), Some(u32::MAX));
        pair.slave.tick(&mut pair.s, wrapped);
        assert!(!pair.slave.is_locked());
        assert_eq!(pair.slave.unlocked_ms(wrapped), Some(u32::MAX));
        assert_eq!(
            pair.slave.unlocked_ms(wrapped.wrapping_add(1 << 31)),
            Some(u32::MAX)
        );
    }

    #[test]
    fn the_holdover_follows_the_config_and_survives_a_reset() {
        let mut pair = Pair::new(2_000.0, 0.0);
        let config = Config {
            holdover: core::time::Duration::from_millis(50),
            ..Config::default()
        };
        pair.run_ms(4000);
        assert!(pair.slave.is_locked());
        pair.slave.set_config(config);
        assert!(pair.slave.is_locked());
        let last_sample = pair.slave.last_sample_ms;
        pair.slave.tick(&mut pair.s, last_sample + 50);
        assert!(pair.slave.is_locked());
        pair.slave.tick(&mut pair.s, last_sample + 51);
        assert!(!pair.slave.is_locked());
        assert_eq!(pair.slave.unlocked_ms(last_sample + 60), Some(9));

        pair.slave.reset(&mut pair.s);
        assert_eq!(pair.slave.config(), config);
        assert_eq!(pair.slave.unlocked_ms(last_sample + 60), None);
    }
}

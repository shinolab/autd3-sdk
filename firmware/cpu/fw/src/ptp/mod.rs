pub mod message;
pub mod servo;

#[cfg(test)]
mod tests;

use crate::net::Mac;
use crate::nic::{Nic, RxMeta, complete};
use message::{
    FRAME_CAP, MSG_DELAY_REQ, MSG_DELAY_RESP, MSG_FOLLOW_UP, MSG_SYNC, Message, Outgoing,
    PortIdentity, correction_ns, port_identity,
};
use servo::Servo;

pub const SYNC_INTERVAL_MS: u32 = 16;
pub const TX_TIMESTAMP_TIMEOUT_MS: u32 = 3;
pub const DELAY_RESP_TIMEOUT_MS: u32 = 6;
pub const HOLDOVER_MS: u32 = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Master,
    Slave,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    None,
    Locked,
    Stepped,
    Lost,
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
enum SlaveState {
    Idle,
    GotSync {
        seq: u16,
        t2: u64,
        correction: i64,
    },
    WaitReqTs {
        t1: u64,
        t2: u64,
        correction: i64,
        since: u32,
    },
    WaitResp {
        t1: u64,
        t2: u64,
        t3: u64,
        correction: i64,
        since: u32,
    },
}

pub struct Ptp {
    role: Option<Role>,
    mac: Mac,
    clock_id: [u8; 8],
    upstream: u8,
    downstream: u8,
    seq: u16,
    last_sync_ms: u32,
    master: MasterState,
    slave: SlaveState,
    servo: Servo,
    locked: bool,
    last_exchange_ms: u32,
    applied_ppb: Option<i32>,
    tx_after: u64,
    last_sync_seq: Option<u16>,
    last_offset: i64,
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
            role: None,
            mac: [0; 6],
            clock_id: [0; 8],
            upstream: 0,
            downstream: 1,
            seq: 0,
            last_sync_ms: 0,
            master: MasterState::Idle,
            slave: SlaveState::Idle,
            servo: Servo::new(),
            locked: false,
            last_exchange_ms: 0,
            applied_ppb: None,
            tx_after: 0,
            last_sync_seq: None,
            last_offset: 0,
            frame: [0; FRAME_CAP],
        }
    }

    pub fn configure<N: Nic>(&mut self, nic: &mut N, role: Role, mac: Mac, upstream: u8) {
        self.reset(nic);
        self.role = Some(role);
        self.mac = mac;
        self.clock_id = autd3_cpu_wire::udp::eui64(mac);
        self.upstream = upstream & 1;
        self.downstream = crate::nic::other_port(self.upstream);
    }

    pub fn reset<N: Nic>(&mut self, nic: &mut N) {
        *self = Self::new();
        self.apply_drift(nic, 0);
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
    pub const fn last_offset(&self) -> i64 {
        self.last_offset
    }

    pub fn time_set(&mut self) {
        if self.role == Some(Role::Master) {
            self.locked = true;
            self.master = MasterState::Idle;
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

    pub fn tick<N: Nic>(&mut self, nic: &mut N, now_ms: u32) -> Event {
        match self.role {
            Some(Role::Master) => {
                self.tick_master(nic, now_ms);
                Event::None
            }
            Some(Role::Slave) => self.tick_slave(nic, now_ms),
            None => Event::None,
        }
    }

    fn tick_master<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        match self.master {
            MasterState::Idle => {
                if !self.locked || now_ms.wrapping_sub(self.last_sync_ms) < SYNC_INTERVAL_MS {
                    return;
                }
                self.last_sync_ms = now_ms;
                self.seq = self.seq.wrapping_add(1);
                let seq = self.seq;
                self.arm_tx_timestamp(nic);
                let sync = Outgoing {
                    kind: MSG_SYNC,
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
                        let follow_up = Outgoing {
                            kind: MSG_FOLLOW_UP,
                            seq,
                            timestamp: t1,
                            correction: 0,
                            requesting: None,
                        };
                        let _ = self.send(nic, &follow_up, self.downstream, false);
                    }
                    TxTimestamp::Stale => self.master = MasterState::Idle,
                    TxTimestamp::Pending
                        if now_ms.wrapping_sub(since) > TX_TIMESTAMP_TIMEOUT_MS =>
                    {
                        self.master = MasterState::Idle;
                    }
                    TxTimestamp::Pending => {}
                }
            }
        }
    }

    fn tick_slave<N: Nic>(&mut self, nic: &mut N, now_ms: u32) -> Event {
        self.poll_delay_req_timestamp(nic, now_ms);
        if let SlaveState::WaitResp { since, .. } = self.slave
            && now_ms.wrapping_sub(since) > DELAY_RESP_TIMEOUT_MS
        {
            self.slave = SlaveState::Idle;
        }
        if self.locked && now_ms.wrapping_sub(self.last_exchange_ms) > HOLDOVER_MS {
            self.locked = false;
            self.servo.unlock();
            return Event::Lost;
        }
        Event::None
    }

    fn apply_drift<N: Nic>(&mut self, nic: &mut N, ppb: i32) {
        if self.applied_ppb != Some(ppb) {
            nic.set_drift(ppb);
            self.applied_ppb = Some(ppb);
        }
    }

    fn poll_delay_req_timestamp<N: Nic>(&mut self, nic: &mut N, now_ms: u32) {
        let SlaveState::WaitReqTs {
            t1,
            t2,
            correction,
            since,
        } = self.slave
        else {
            return;
        };
        match self.take_tx_timestamp(nic, self.upstream) {
            TxTimestamp::At(t3) => {
                self.slave = SlaveState::WaitResp {
                    t1,
                    t2,
                    t3,
                    correction,
                    since: now_ms,
                };
            }
            TxTimestamp::Stale => self.slave = SlaveState::Idle,
            TxTimestamp::Pending if now_ms.wrapping_sub(since) > TX_TIMESTAMP_TIMEOUT_MS => {
                self.slave = SlaveState::Idle;
            }
            TxTimestamp::Pending => {}
        }
    }

    pub fn on_message<N: Nic>(
        &mut self,
        nic: &mut N,
        raw: &[u8],
        rx: RxMeta,
        now_ms: u32,
    ) -> Event {
        let Some(msg) = message::parse(raw) else {
            return Event::None;
        };
        match self.role {
            Some(Role::Master) => {
                if msg.kind == MSG_DELAY_REQ && self.locked {
                    self.answer_delay_req(nic, &msg, rx);
                }
                Event::None
            }
            Some(Role::Slave) => self.on_slave_message(nic, &msg, rx, now_ms),
            None => Event::None,
        }
    }

    fn answer_delay_req<N: Nic>(&mut self, nic: &mut N, msg: &Message, rx: RxMeta) {
        let Some(now) = nic.now() else {
            return;
        };
        let resp = Outgoing {
            kind: MSG_DELAY_RESP,
            seq: msg.seq,
            timestamp: complete(now, rx.timestamp_ns),
            correction: msg.correction,
            requesting: Some(msg.source),
        };
        let _ = self.send(nic, &resp, rx.port, false);
    }

    fn on_slave_message<N: Nic>(
        &mut self,
        nic: &mut N,
        msg: &Message,
        rx: RxMeta,
        now_ms: u32,
    ) -> Event {
        match msg.kind {
            MSG_SYNC if rx.port == self.upstream && self.last_sync_seq != Some(msg.seq) => {
                self.last_sync_seq = Some(msg.seq);
                let Some(now) = nic.now() else {
                    return Event::None;
                };
                self.slave = SlaveState::GotSync {
                    seq: msg.seq,
                    t2: complete(now, rx.timestamp_ns),
                    correction: msg.correction,
                };
                Event::None
            }
            MSG_FOLLOW_UP if rx.port == self.upstream => {
                if let SlaveState::GotSync {
                    seq,
                    t2,
                    correction,
                } = self.slave
                    && seq == msg.seq
                {
                    self.send_delay_req(
                        nic,
                        msg.timestamp,
                        t2,
                        correction.wrapping_add(msg.correction),
                        now_ms,
                    );
                }
                Event::None
            }
            MSG_DELAY_RESP if msg.requesting == Some(self.identity()) => {
                self.poll_delay_req_timestamp(nic, now_ms);
                let SlaveState::WaitResp {
                    t1,
                    t2,
                    t3,
                    correction,
                    ..
                } = self.slave
                else {
                    return Event::None;
                };
                if msg.seq != self.seq {
                    return Event::None;
                }
                self.slave = SlaveState::Idle;
                self.last_exchange_ms = now_ms;
                self.exchange(nic, [t1, t2, t3, msg.timestamp], correction, msg.correction)
            }
            _ => Event::None,
        }
    }

    fn send_delay_req<N: Nic>(
        &mut self,
        nic: &mut N,
        t1: u64,
        t2: u64,
        correction: i64,
        now_ms: u32,
    ) {
        self.seq = self.seq.wrapping_add(1);
        self.arm_tx_timestamp(nic);
        let req = Outgoing {
            kind: MSG_DELAY_REQ,
            seq: self.seq,
            timestamp: 0,
            correction: 0,
            requesting: None,
        };
        self.slave = if self.send(nic, &req, self.upstream, true) {
            SlaveState::WaitReqTs {
                t1,
                t2,
                correction,
                since: now_ms,
            }
        } else {
            SlaveState::Idle
        };
    }

    fn exchange<N: Nic>(
        &mut self,
        nic: &mut N,
        [t1, t2, t3, t4]: [u64; 4],
        corr_ms: i64,
        corr_sm: i64,
    ) -> Event {
        let forward = t2.cast_signed() - t1.cast_signed() - correction_ns(corr_ms);
        let backward = t4.cast_signed() - t3.cast_signed() - correction_ns(corr_sm);
        let offset = (forward - backward) / 2;
        self.last_offset = offset;
        let out = self.servo.sample(offset, t2.cast_signed());
        if let Some(ppb) = out.drift_ppb {
            self.apply_drift(nic, ppb);
        }
        if let Some(step) = out.step_ns {
            nic.stop_pulse();
            let _ = nic.step(step);
            self.locked = false;
            self.slave = SlaveState::Idle;
            return Event::Stepped;
        }
        if out.locked_now {
            self.locked = true;
            return Event::Locked;
        }
        Event::None
    }
}

use std::vec::Vec;

use crate::net::Mac;
use crate::nic::{NS_PER_SEC, Nic, TxStamp};

pub(crate) struct SimClock {
    base: i128,
    frac: f64,
    anchor: f64,
    crystal_ppb: f64,
    drift_ppb: f64,
}

impl SimClock {
    pub(crate) fn new(start: f64, crystal_ppb: f64) -> Self {
        Self {
            base: start as i128,
            frac: start.fract(),
            anchor: 0.0,
            crystal_ppb,
            drift_ppb: 0.0,
        }
    }

    fn elapsed(&self, t: f64) -> f64 {
        self.frac + (t - self.anchor) * (1.0 + (self.crystal_ppb + self.drift_ppb) * 1e-9)
    }

    pub(crate) fn at(&self, t: f64) -> i128 {
        self.base + self.elapsed(t).floor() as i128
    }

    fn reanchor(&mut self, t: f64) {
        let elapsed = self.elapsed(t);
        let whole = elapsed.floor();
        self.base += whole as i128;
        self.frac = elapsed - whole;
        self.anchor = t;
    }
}

pub(crate) struct Sent {
    pub(crate) frame: Vec<u8>,
    pub(crate) port: u8,
    pub(crate) at: f64,
    pub(crate) timestamp: bool,
}

pub(crate) struct SimNic {
    pub(crate) clock: SimClock,
    pub(crate) t: f64,
    pub(crate) sent: Vec<Sent>,
    tx_stamps: [Option<TxStamp>; 2],
    pub(crate) steps: u32,
    pub(crate) steps_with_pulse: u32,
    pub(crate) drift_ppb: i32,
    pub(crate) drift_writes: u32,
    pulse_running: bool,
    pub(crate) time_set: Option<u64>,
    pub(crate) forwarding_open: Option<bool>,
    pub(crate) mac: Option<Mac>,
    pub(crate) link: bool,
    pub(crate) pulse_armed: u32,
    pub(crate) pulse_stopped: u32,
    pub(crate) pulse_ready: bool,
}

impl SimNic {
    pub(crate) fn with_clock(clock: SimClock) -> Self {
        Self {
            clock,
            t: 0.0,
            sent: Vec::new(),
            tx_stamps: [None; 2],
            steps: 0,
            steps_with_pulse: 0,
            drift_ppb: 0,
            drift_writes: 0,
            pulse_running: false,
            time_set: None,
            forwarding_open: None,
            mac: None,
            link: false,
            pulse_armed: 0,
            pulse_stopped: 0,
            pulse_ready: false,
        }
    }

    pub(crate) fn local(&self) -> u64 {
        self.local_at(self.t)
    }

    pub(crate) fn local_at(&self, t: f64) -> u64 {
        u64::try_from(self.clock.at(t)).unwrap_or(0)
    }

    pub(crate) fn jump(&mut self, offset_ns: i64) {
        self.clock.reanchor(self.t);
        self.clock.base += i128::from(offset_ns);
    }

    pub(crate) fn stamp_tx(&mut self, port: u8, ns: u32) {
        let overwritten = self.tx_stamps[usize::from(port & 1)].is_some();
        self.tx_stamps[usize::from(port & 1)] = Some(TxStamp { ns, overwritten });
    }
}

impl Nic for SimNic {
    fn send(&mut self, frame: &[u8], port: u8, timestamp: bool) -> bool {
        if timestamp {
            self.stamp_tx(port, (self.local() % NS_PER_SEC) as u32);
        }
        self.sent.push(Sent {
            frame: frame.to_vec(),
            port,
            at: self.t,
            timestamp,
        });
        true
    }

    fn now(&mut self) -> Option<u64> {
        Some(self.local())
    }

    fn step(&mut self, offset_ns: i64) -> bool {
        self.clock.reanchor(self.t);
        self.clock.base += i128::from(offset_ns);
        self.steps += 1;
        if self.pulse_running {
            self.steps_with_pulse += 1;
        }
        true
    }

    fn set_time(&mut self, ns: u64) -> bool {
        self.clock.base = i128::from(ns);
        self.clock.frac = 0.0;
        self.clock.anchor = self.t;
        self.time_set = Some(ns);
        true
    }

    fn set_drift(&mut self, ppb: i32) {
        self.clock.reanchor(self.t);
        self.clock.drift_ppb = f64::from(ppb);
        self.drift_ppb = ppb;
        self.drift_writes += 1;
    }

    fn clear_tx_timestamps(&mut self) {
        self.tx_stamps = [None; 2];
    }

    fn take_tx_timestamp(&mut self, port: u8) -> Option<TxStamp> {
        self.tx_stamps[usize::from(port & 1)].take()
    }

    fn set_forwarding(&mut self, open: bool) {
        self.forwarding_open = Some(open);
    }

    fn set_mac(&mut self, mac: Mac) {
        self.mac = Some(mac);
    }

    fn downstream_link(&mut self, _port: u8) -> bool {
        self.link
    }

    fn arm_pulse(&mut self) {
        self.pulse_armed += 1;
        self.pulse_running = true;
    }

    fn stop_pulse(&mut self) {
        self.pulse_stopped += 1;
        self.pulse_running = false;
        self.pulse_ready = false;
    }

    fn pulse_ready(&mut self) -> bool {
        self.pulse_ready
    }
}

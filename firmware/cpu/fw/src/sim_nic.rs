use std::vec::Vec;

use crate::net::Mac;
use crate::nic::{NS_PER_SEC, Nic, TxStamp};

const CLOCK_PERIOD_NS: f64 = 10.0;
const CLOCKS_PER_PPB: u64 = 100_000_000;

pub(crate) struct SimClock {
    base: i128,
    frac: f64,
    anchor: f64,
    crystal_ppb: f64,
    correction_started: f64,
    running_period_clocks: u64,
    written_period_clocks: u64,
    correction_ns: i128,
}

impl SimClock {
    pub(crate) fn new(start: f64, crystal_ppb: f64) -> Self {
        Self {
            base: start as i128,
            frac: start.fract(),
            anchor: 0.0,
            crystal_ppb,
            correction_started: 0.0,
            running_period_clocks: 0,
            written_period_clocks: 0,
            correction_ns: 0,
        }
    }

    fn elapsed(&self, t: f64) -> f64 {
        self.frac + (t - self.anchor) * (1.0 + self.crystal_ppb * 1e-9)
    }

    fn correction_count(&self, t: f64) -> i128 {
        if self.running_period_clocks == 0 || t <= self.correction_started {
            return 0;
        }
        let clocks = ((t - self.correction_started) / CLOCK_PERIOD_NS).floor() as i128;
        let running = i128::from(self.running_period_clocks);
        if clocks < running {
            return 0;
        }
        match self.written_period_clocks {
            0 => 1,
            written => 1 + (clocks - running) / i128::from(written),
        }
    }

    pub(crate) fn at(&self, t: f64) -> i128 {
        self.base + self.elapsed(t).floor() as i128 + self.correction_count(t) * self.correction_ns
    }

    fn reanchor(&mut self, t: f64) {
        let elapsed = self.elapsed(t);
        let whole = elapsed.floor();
        self.base += whole as i128;
        self.frac = elapsed - whole;
        self.anchor = t;
    }

    fn settle_corrections(&mut self, t: f64) {
        let count = self.correction_count(t);
        if count == 0 {
            return;
        }
        self.base += count * self.correction_ns;
        let clocks = i128::from(self.running_period_clocks)
            + (count - 1) * i128::from(self.written_period_clocks);
        self.correction_started += clocks as f64 * CLOCK_PERIOD_NS;
        self.running_period_clocks = self.written_period_clocks;
    }

    fn set_drift(&mut self, t: f64, ppb: i32) {
        self.settle_corrections(t);
        let magnitude = u64::from(ppb.unsigned_abs());
        self.written_period_clocks = (CLOCKS_PER_PPB + magnitude / 2)
            .checked_div(magnitude)
            .map_or(0, |period| period.max(1));
        self.correction_ns = i128::from(ppb.signum());
        if self.running_period_clocks == 0 {
            self.running_period_clocks = self.written_period_clocks;
            self.correction_started = t;
        }
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
    pub(crate) pulse_running: bool,
    pub(crate) time_set: Option<u64>,
    pub(crate) forwarding_open: Option<bool>,
    pub(crate) mac: Option<Mac>,
    pub(crate) link: bool,
    pub(crate) pulse_armed: u32,
    pub(crate) pulse_arm_failures: u32,
    pub(crate) pulse_arm_failed: u32,
    pub(crate) pulse_stopped: u32,
    pub(crate) pulse_ready: bool,
    pub(crate) send_refusals: u32,
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
            pulse_arm_failures: 0,
            pulse_arm_failed: 0,
            pulse_stopped: 0,
            pulse_ready: false,
            send_refusals: 0,
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
        if self.send_refusals > 0 {
            self.send_refusals -= 1;
            return false;
        }
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
        self.clock.settle_corrections(self.t);
        self.clock.base = i128::from(ns);
        self.clock.frac = 0.0;
        self.clock.anchor = self.t;
        self.time_set = Some(ns);
        true
    }

    fn set_drift(&mut self, ppb: i32) {
        self.clock.set_drift(self.t, ppb);
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

    fn arm_pulse(&mut self) -> bool {
        if self.pulse_arm_failures > 0 {
            self.pulse_arm_failures -= 1;
            self.pulse_arm_failed += 1;
            self.pulse_running = false;
            self.pulse_ready = false;
            return false;
        }
        self.pulse_armed += 1;
        self.pulse_running = true;
        true
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

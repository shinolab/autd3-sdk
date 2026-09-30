use std::vec::Vec;

use super::message::{self, FRAME_CAP, MSG_DELAY_RESP, MSG_SYNC, Outgoing};
use super::servo::{LOCK_SAMPLES, STEP_THRESHOLD_NS, Servo};
use super::{Event, Ptp, Role};
use crate::net::{ETH_HEADER, Mac};
use crate::nic::{NS_PER_SEC, RxMeta};
use crate::sim_nic::{Sent, SimClock, SimNic};

const MASTER_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x00];
const SLAVE_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x01];
const WIRE_DELAY_NS: f64 = 700.0;
const HOP_NS: f64 = 10_000.0;

struct Pair {
    master: Ptp,
    slave: Ptp,
    m: SimNic,
    s: SimNic,
    events: Vec<Event>,
    now_ms: u32,
}

impl Pair {
    fn new(slave_offset_ns: f64, slave_crystal_ppb: f64) -> Self {
        let start = 1_000_000_000_000.0;
        let mut m = SimNic::with_clock(SimClock::new(start, 0.0));
        let mut s = SimNic::with_clock(SimClock::new(start + slave_offset_ns, slave_crystal_ppb));
        let mut master = Ptp::new();
        let mut slave = Ptp::new();
        master.configure(&mut m, Role::Master, MASTER_MAC, 0);
        slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
        master.time_set();
        Self {
            master,
            slave,
            m,
            s,
            events: Vec::new(),
            now_ms: 0,
        }
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
                    continue;
                }
                let rx = RxMeta {
                    port: 0,
                    timestamp_ns: (self.s.local_at(f.at + WIRE_DELAY_NS) % NS_PER_SEC) as u32,
                };
                let now_ms = self.now_ms;
                let e = self
                    .slave
                    .on_message(&mut self.s, &f.frame[ETH_HEADER..], rx, now_ms);
                self.events.push(e);
            }
            for f in from_slave {
                if f.port != 0 {
                    continue;
                }
                let rx = RxMeta {
                    port: 1,
                    timestamp_ns: (self.m.local_at(f.at + WIRE_DELAY_NS) % NS_PER_SEC) as u32,
                };
                let now_ms = self.now_ms;
                let e = self
                    .master
                    .on_message(&mut self.m, &f.frame[ETH_HEADER..], rx, now_ms);
                self.events.push(e);
            }
        }
    }

    fn run_ms(&mut self, ms: u32) {
        for _ in 0..ms {
            self.m.t += 1e6;
            self.s.t = self.m.t;
            self.now_ms += 1;
            let now_ms = self.now_ms;
            let e = self.master.tick(&mut self.m, now_ms);
            self.events.push(e);
            let e = self.slave.tick(&mut self.s, now_ms);
            self.events.push(e);
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
    let steps = pair.events.iter().filter(|e| **e == Event::Stepped).count();
    let locks = pair.events.iter().filter(|e| **e == Event::Locked).count();
    assert_eq!(steps, 1);
    assert_eq!(locks, 1);
    assert!(pair.slave.is_locked());
    assert!(pair.true_offset().abs() < 100.0, "{}", pair.true_offset());
    assert!(pair.slave.last_offset().abs() < 100);
}

#[test]
fn a_small_initial_offset_locks_without_a_step() {
    let mut pair = Pair::new(2_000.0, -15_000.0);
    pair.run_ms(4000);
    assert!(!pair.events.contains(&Event::Stepped));
    assert!(pair.events.contains(&Event::Locked));
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
fn a_master_is_silent_until_its_time_is_set() {
    let mut m = SimNic::with_clock(SimClock::new(0.0, 0.0));
    let mut master = Ptp::new();
    master.configure(&mut m, Role::Master, MASTER_MAC, 0);
    for ms in 0..100 {
        master.tick(&mut m, ms);
    }
    assert!(m.sent.is_empty());
    master.time_set();
    master.tick(&mut m, 100);
    assert_eq!(m.sent.len(), 1);
    assert_eq!(m.sent[0].port, 1);
    assert_eq!(m.sent[0].frame[ETH_HEADER] & 0x0F, MSG_SYNC);
}

#[test]
fn a_sync_from_the_downstream_port_is_ignored() {
    let mut s = SimNic::with_clock(SimClock::new(0.0, 0.0));
    let mut slave = Ptp::new();
    slave.configure(&mut s, Role::Slave, SLAVE_MAC, 0);
    let mut buf = [0u8; FRAME_CAP];
    let sync = Outgoing {
        kind: MSG_SYNC,
        seq: 1,
        timestamp: 0,
        correction: 0,
        requesting: None,
    };
    let len = message::build(&mut buf, MASTER_MAC, [0; 8], &sync);
    let follow_up = Outgoing {
        kind: super::message::MSG_FOLLOW_UP,
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

#[test]
fn a_delay_resp_for_another_slave_is_ignored() {
    let mut pair = Pair::new(0.0, 0.0);
    pair.run_ms(40);
    let mut buf = [0u8; FRAME_CAP];
    let foreign = Outgoing {
        kind: MSG_DELAY_RESP,
        seq: pair.slave.seq,
        timestamp: 0,
        correction: 0,
        requesting: Some([9; 10]),
    };
    let len = message::build(&mut buf, MASTER_MAC, [0; 8], &foreign);
    let before = pair.slave.last_offset();
    let e = pair.slave.on_message(
        &mut pair.s,
        &buf[ETH_HEADER..len],
        RxMeta {
            port: 0,
            timestamp_ns: 0,
        },
        40,
    );
    assert_eq!(e, Event::None);
    assert_eq!(pair.slave.last_offset(), before);
}

#[test]
fn messages_round_trip() {
    let mut buf = [0u8; FRAME_CAP];
    let out = Outgoing {
        kind: MSG_DELAY_RESP,
        seq: 0xBEEF,
        timestamp: 812_345_678_901_234_567,
        correction: -(5 << 16),
        requesting: Some([1, 2, 3, 4, 5, 6, 7, 8, 0, 1]),
    };
    let len = message::build(&mut buf, SLAVE_MAC, [7; 8], &out);
    assert_eq!(len, 68);
    assert_eq!(&buf[0..6], &autd3_cpu_wire::udp::PTP_MULTICAST_MAC);
    assert_eq!(&buf[12..14], &[0x88, 0xF7]);
    let m = message::parse(&buf[ETH_HEADER..len]).unwrap();
    assert_eq!(m.kind, MSG_DELAY_RESP);
    assert_eq!(m.seq, 0xBEEF);
    assert_eq!(m.timestamp, 812_345_678_901_234_567);
    assert_eq!(message::correction_ns(m.correction), -5);
    assert_eq!(m.requesting, Some([1, 2, 3, 4, 5, 6, 7, 8, 0, 1]));
    assert_eq!(&m.source[..8], &[7; 8]);

    let sync = Outgoing {
        kind: MSG_SYNC,
        seq: 1,
        timestamp: 0,
        correction: 0,
        requesting: None,
    };
    let len = message::build(&mut buf, SLAVE_MAC, [7; 8], &sync);
    assert_eq!(len, 60);
    assert_eq!(buf[ETH_HEADER + 6], 0x02);
    assert_eq!(
        message::parse(&buf[ETH_HEADER..len]).unwrap().requesting,
        None
    );
    assert_eq!(message::parse(&buf[ETH_HEADER..ETH_HEADER + 20]), None);
    let mut v1 = buf;
    v1[ETH_HEADER + 1] = 1;
    assert_eq!(message::parse(&v1[ETH_HEADER..len]), None);
}

#[test]
fn the_servo_needs_64_quiet_samples_to_lock() {
    let mut servo = Servo::new();
    let mut at = 0i64;
    for i in 0..LOCK_SAMPLES {
        at += 16_000_000;
        let out = servo.sample(50, at);
        assert_eq!(out.locked_now, i + 1 == LOCK_SAMPLES);
        assert!(out.step_ns.is_none());
    }
    assert!(servo.is_locked());
    at += 16_000_000;
    let out = servo.sample(STEP_THRESHOLD_NS + 1, at);
    assert_eq!(out.step_ns, Some(-(STEP_THRESHOLD_NS + 1)));
    assert!(out.unlocked);
    assert!(!servo.is_locked());
}

#[test]
fn the_servo_estimates_the_frequency_before_its_first_step() {
    let mut servo = Servo::new();
    assert_eq!(servo.sample(1_000_000, 0), super::servo::Output::default());
    let out = servo.sample(1_016_000, 16_000_000);
    assert_eq!(out.step_ns, Some(-1_016_000));
    assert_eq!(out.drift_ppb, Some(-500_000));
    let mut servo = Servo::new();
    servo.sample(1_000_000, 0);
    let out = servo.sample(1_000_160, 16_000_000);
    assert_eq!(out.drift_ppb, Some(-10_000));
}

#[test]
fn a_moderate_offset_pulls_the_frequency_proportionally() {
    let mut servo = Servo::new();
    let out = servo.sample(1_600, 16_000_000);
    assert_eq!(out.drift_ppb, Some(-12_000));
}

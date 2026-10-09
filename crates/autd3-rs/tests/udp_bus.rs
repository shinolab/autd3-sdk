use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use autd3_rs::DeviceState;
use autd3_rs::protocol::{Cmd, FRAME_BYTES_MAX};
use autd3_rs::udp::{Reply, TransportOption, UdpBus, UdpError};
use autd3_rs::value::SysTime;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

mod common;

fn frames(n: usize, seq: u8, cmd: Cmd) -> Vec<Vec<u8>> {
    vec![vec![seq, cmd.as_u8()]; n]
}

fn collect(bus: &mut UdpBus, msg_id: u16, n: usize) -> Vec<Reply> {
    collect_within(bus, msg_id, n, Duration::from_millis(500))
}

fn collect_within(bus: &mut UdpBus, msg_id: u16, n: usize, wait: Duration) -> Vec<Reply> {
    let deadline = Instant::now() + wait;
    let mut replies: Vec<Reply> = Vec::new();
    while replies.len() < n {
        let Some(reply) = bus.recv(deadline).unwrap() else {
            break;
        };
        if reply.msg_id == msg_id {
            replies.push(reply);
        }
    }
    replies.sort_by_key(|r| r.device);
    replies
}

fn open(emulator: &UdpEmulator, n: usize) -> UdpBus {
    UdpBus::open(&common::option(emulator), n).unwrap()
}

#[test]
fn units_are_enumerated_in_chain_order() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let bus = open(&emulator, 3);
    assert_eq!(bus.num_devices(), 3);
    for (i, addr) in bus.units().iter().enumerate() {
        assert_eq!(*addr, emulator.device_addr(i));
    }
}

fn time_to_send_heartbeats(option: &TransportOption, count: u32) -> Duration {
    let bus = UdpBus::open(option, 1).unwrap();
    let start = Instant::now();
    for _ in 0..count {
        bus.heartbeat().unwrap();
    }
    start.elapsed()
}

#[test]
fn a_send_rate_limit_spaces_the_datagrams_out() {
    const HEARTBEAT_WIRE_BITS: u64 = (4 + 86) * 8;
    const PERCENT: f32 = 3.2;
    const RATE: u32 = 3_200_000;
    const COUNT: u32 = 400;
    const BURST_ALLOWANCE: Duration = Duration::from_millis(4);
    let emulator = UdpEmulator::spawn(1).unwrap();
    let limited = TransportOption {
        send_rate_limit: Some(PERCENT),
        ..common::option(&emulator)
    };
    let on_the_wire = Duration::from_nanos(
        u64::from(COUNT) * HEARTBEAT_WIRE_BITS * 1_000_000_000 / u64::from(RATE),
    );
    let elapsed = time_to_send_heartbeats(&limited, COUNT);
    assert!(elapsed + BURST_ALLOWANCE >= on_the_wire, "{elapsed:?}");
}

#[test]
fn a_send_rate_limit_below_the_minimum_fails_the_open() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let option = TransportOption {
        send_rate_limit: Some(0.95),
        ..common::option(&emulator)
    };
    assert!(matches!(
        UdpBus::open(&option, 1),
        Err(UdpError::InvalidSendRateLimit { .. })
    ));
}

#[test]
fn a_geometry_larger_than_the_chain_times_out() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = TransportOption {
        enumeration_timeout: Duration::from_millis(200),
        ..common::option(&emulator)
    };
    match UdpBus::open(&option, 3) {
        Err(UdpError::EnumerationTimeout { expected, found }) => {
            assert_eq!((expected, found), (3, 2));
        }
        other => panic!("expected an enumeration timeout, got {:?}", other.err()),
    }
}

#[test]
fn a_geometry_smaller_than_the_chain_is_rejected() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    match UdpBus::open(&common::option(&emulator), 2) {
        Err(UdpError::DeviceCountMismatch { expected, found }) => {
            assert_eq!((expected, found), (2, 3));
        }
        other => panic!("expected a count mismatch, got {:?}", other.err()),
    }
}

#[test]
fn a_zero_device_count_is_rejected() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    assert!(matches!(
        UdpBus::open(&common::option(&emulator), 0),
        Err(UdpError::InvalidDeviceCount(0))
    ));
}

#[test]
fn every_frame_is_answered_once_with_its_result() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);

    let msg_id = bus.send(&frames(2, 0, Cmd::Reset)).unwrap().msg_id;
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.len(), 2);
    assert!(replies.iter().all(|r| r.ack == 0xFF));

    let msg_id = bus.send(&frames(2, 0, Cmd::Nop)).unwrap().msg_id;
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.iter().map(|r| r.device).collect::<Vec<_>>(), [0, 1]);
    assert!(
        replies
            .iter()
            .all(|r| r.ack == 0 && r.status == 0 && r.data().is_empty())
    );
}

#[test]
fn a_heartbeat_reports_the_last_result() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);
    let msg_id = bus.send(&frames(2, 0, Cmd::Reset)).unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);
    let msg_id = bus.send(&frames(2, 0, Cmd::Nop)).unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);

    let msg_id = bus.heartbeat().unwrap().msg_id;
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.len(), 2);
    assert!(replies.iter().all(|r| r.ack == 0));
    assert_eq!(bus.stats().heartbeats(), 1);
}

#[test]
fn frames_of_the_wrong_count_or_length_are_rejected() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let bus = open(&emulator, 2);
    assert!(matches!(
        bus.send(&frames(1, 0, Cmd::Nop)),
        Err(UdpError::FrameCountMismatch {
            expected: 2,
            got: 1
        })
    ));
    assert!(matches!(
        bus.send(&[[0u8; 1], [0u8; 1]]),
        Err(UdpError::InvalidFrameLength(1))
    ));
}

#[test]
fn the_device_time_counts_from_the_open_and_is_known_at_once() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut bus = open(&emulator, 1);
    let clock = bus.device_clock();
    let at_open = clock.now().expect("SetTime seeds the device clock");
    for _ in 0..3 {
        let msg_id = bus.heartbeat().unwrap().msg_id;
        assert_eq!(collect(&mut bus, msg_id, 1).len(), 1);
    }
    let later = clock.now().expect("observed");
    assert!(later >= at_open);
    assert!(later < SysTime::ZERO + Duration::from_secs(60));
}

fn heartbeat_states(bus: &mut UdpBus, n: usize) -> Vec<DeviceState> {
    let checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap().msg_id;
    assert_eq!(collect(bus, msg_id, n).len(), n);
    checker.check().unwrap().devices().to_vec()
}

#[test]
fn every_unit_is_locked_to_the_grandmaster_when_open_returns() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut bus = open(&emulator, 3);
    assert_eq!(heartbeat_states(&mut bus, 3), [DeviceState::Ready; 3]);
}

#[test]
fn a_unit_that_does_not_lock_fails_the_open_with_a_sync_timeout() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    emulator.set_ptp_lock_blocked(2, true);
    let option = TransportOption {
        sync_timeout: Duration::from_millis(300),
        ..common::option(&emulator)
    };
    match UdpBus::open(&option, 3) {
        Err(UdpError::SyncTimeout { not_ready, timeout }) => {
            assert_eq!(not_ready, [2]);
            assert_eq!(timeout, Duration::from_millis(300));
        }
        other => panic!("expected a sync timeout, got {:?}", other.err()),
    }

    emulator.set_ptp_lock_blocked(2, false);
    let mut bus = open(&emulator, 3);
    assert_eq!(heartbeat_states(&mut bus, 3), [DeviceState::Ready; 3]);
}

#[test]
fn an_unsynchronized_open_skips_the_lock_wait() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    for unit in 1..3 {
        emulator.set_ptp_lock_blocked(unit, true);
    }
    let option = TransportOption {
        sync_timeout: Duration::ZERO,
        ..common::option(&emulator)
    };
    let mut bus = UdpBus::open_unsynchronized(&option, 3).unwrap();
    assert_eq!(bus.num_devices(), 3);
    let msg_id = bus.send(&frames(3, 0, Cmd::Reset)).unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 3).len(), 3);
    assert_eq!(heartbeat_states(&mut bus, 3), [DeviceState::Syncing; 3]);
}

#[test]
fn a_unit_that_loses_its_lock_is_syncing_until_it_locks_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);
    assert_eq!(heartbeat_states(&mut bus, 2), [DeviceState::Ready; 2]);

    emulator.set_ptp_lock_blocked(1, true);
    assert_eq!(
        heartbeat_states(&mut bus, 2),
        [DeviceState::Ready, DeviceState::Syncing]
    );
    emulator.set_ptp_lock_blocked(0, true);
    assert_eq!(
        heartbeat_states(&mut bus, 2),
        [DeviceState::Ready, DeviceState::Syncing]
    );

    emulator.set_ptp_lock_blocked(1, false);
    let deadline = Instant::now() + Duration::from_secs(2);
    while heartbeat_states(&mut bus, 2) != [DeviceState::Ready; 2] {
        assert!(Instant::now() < deadline, "the unit did not lock again");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_rebooted_middle_device_is_lost_with_everything_behind_it() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let option = TransportOption {
        lost_timeout: Duration::from_millis(100),
        heartbeat: Some(Duration::from_millis(10)),
        ..common::option(&emulator)
    };
    let mut bus = UdpBus::open(&option, 3).unwrap();
    let checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 3).len(), 3);
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 3]);

    emulator.reboot(1);
    let until = Instant::now() + Duration::from_millis(250);
    while Instant::now() < until {
        let msg_id = bus.heartbeat().unwrap().msg_id;
        let _ = collect_within(&mut bus, msg_id, 3, Duration::from_millis(10));
    }
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Lost, DeviceState::Lost]
    );
    for _ in 0..3 {
        let msg_id = bus.heartbeat().unwrap().msg_id;
        let replies = collect_within(&mut bus, msg_id, 1, Duration::from_millis(100));
        assert_eq!(replies.iter().map(|r| r.device).collect::<Vec<_>>(), [0]);
    }

    bus.close().unwrap();
    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    drop(bus);

    let mut bus = open(&emulator, 3);
    let checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 3).len(), 3);
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 3]);
}

#[test]
fn a_host_stall_past_the_lost_timeout_reads_queued_replies_before_judging() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = common::option(&emulator);
    let mut bus = UdpBus::open(&option, 2).unwrap();
    let checker = bus.state_checker();

    bus.heartbeat().unwrap();
    std::thread::sleep(option.lost_timeout * 2);
    let msg_id = bus.heartbeat().unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);

    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );
}

#[test]
fn a_host_silent_past_the_lost_timeout_without_requests_keeps_the_devices() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = common::option(&emulator);
    let mut bus = UdpBus::open(&option, 2).unwrap();
    let checker = bus.state_checker();

    std::thread::sleep(option.lost_timeout * 3);
    assert!(bus.recv(Instant::now()).unwrap().is_none());
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );

    let msg_id = bus.heartbeat().unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );
}

#[test]
fn a_unit_waiting_for_its_turn_under_a_send_rate_limit_is_not_lost() {
    const UNITS: usize = 16;
    let emulator = UdpEmulator::spawn(UNITS).unwrap();
    let option = TransportOption {
        heartbeat: None,
        lost_timeout: Duration::from_millis(40),
        send_rate_limit: Some(3.2),
        ..common::option(&emulator)
    };
    let mut bus = UdpBus::open(&option, UNITS).unwrap();
    let checker = bus.state_checker();
    let msg_id = bus.send(&frames(UNITS, 0, Cmd::Reset)).unwrap().msg_id;
    assert_eq!(collect(&mut bus, msg_id, UNITS).len(), UNITS);

    let mut frame = vec![0u8; FRAME_BYTES_MAX];
    frame[1] = Cmd::Nop.as_u8();
    let msg_id = bus.next_msg_id();
    let sending = AtomicBool::new(true);
    let (sent, replied, elapsed) = std::thread::scope(|scope| {
        let receiver = scope.spawn(|| {
            let mut replied = 0u128;
            let mut quiet_since = None;
            loop {
                let now = Instant::now();
                if let Some(reply) = bus.recv(now + Duration::from_millis(1)).unwrap() {
                    if reply.msg_id == msg_id {
                        replied |= 1 << reply.device;
                    }
                    continue;
                }
                if sending.load(Ordering::Acquire) {
                    continue;
                }
                let since = *quiet_since.get_or_insert(now);
                if replied.count_ones() as usize == UNITS
                    || now - since > Duration::from_millis(500)
                {
                    break replied;
                }
            }
        });
        let start = Instant::now();
        let sent = bus.send_broadcast(&frame).unwrap();
        let elapsed = start.elapsed();
        sending.store(false, Ordering::Release);
        (sent, receiver.join().unwrap(), elapsed)
    });

    assert!(elapsed > option.lost_timeout, "{elapsed:?}");
    assert_eq!(sent.devices, (1 << UNITS) - 1);
    assert_eq!(replied, (1 << UNITS) - 1);
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready; UNITS]
    );
}

use std::time::{Duration, Instant};

use autd3_rs::DeviceState;
use autd3_rs::protocol::{Cmd, FRAME_BYTES_MAX, Seq, TxFrame};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::udp::{Reply, TransportOption, UdpBus, UdpError};

fn frames(n: usize, seq: u8, cmd: Cmd) -> Vec<[u8; FRAME_BYTES_MAX]> {
    let mut tx = vec![[0u8; FRAME_BYTES_MAX]; n];
    for buf in &mut tx {
        TxFrame::new(Seq::new(seq), cmd).write_to(buf);
    }
    tx
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
    UdpBus::open(&emulator.option(), n).unwrap()
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

#[test]
fn a_geometry_larger_than_the_chain_times_out() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = TransportOption {
        enumeration_timeout: Duration::from_millis(200),
        ..emulator.option()
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
    match UdpBus::open(&emulator.option(), 2) {
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
        UdpBus::open(&emulator.option(), 0),
        Err(UdpError::InvalidDeviceCount(0))
    ));
}

#[test]
fn every_frame_is_answered_once_with_its_result() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);

    let msg_id = bus.send(&frames(2, 0, Cmd::Reset)).unwrap();
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.len(), 2);
    assert!(replies.iter().all(|r| r.ack == 0xFF));

    let msg_id = bus.send(&frames(2, 0, Cmd::ReadErrorDetail)).unwrap();
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.iter().map(|r| r.device).collect::<Vec<_>>(), [0, 1]);
    assert!(
        replies
            .iter()
            .all(|r| r.ack == 0 && r.status == 0 && r.data() == [0])
    );
}

#[test]
fn a_heartbeat_reports_the_last_result() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);
    let msg_id = bus.send(&frames(2, 0, Cmd::Reset)).unwrap();
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);
    let msg_id = bus.send(&frames(2, 0, Cmd::Nop)).unwrap();
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);

    let msg_id = bus.heartbeat().unwrap();
    let replies = collect(&mut bus, msg_id, 2);
    assert_eq!(replies.len(), 2);
    assert!(replies.iter().all(|r| r.ack == 0));
    assert_eq!(bus.stats().heartbeats(), 1);
}

#[test]
fn frames_of_the_wrong_count_or_length_are_rejected() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);
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
fn replies_feed_the_device_clock() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut bus = open(&emulator, 1);
    let clock = bus.device_clock();
    assert_eq!(clock.observation(), None);
    for _ in 0..3 {
        let msg_id = bus.heartbeat().unwrap();
        assert_eq!(collect(&mut bus, msg_id, 1).len(), 1);
    }
    let observation = clock.observation().expect("observed");
    assert_eq!(observation.samples, 3);
    assert!(observation.offset_ns.abs() < 1_000_000_000);
}

fn heartbeat_states(bus: &mut UdpBus, n: usize) -> Vec<DeviceState> {
    let mut checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap();
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
        ..emulator.option()
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
        ..emulator.option()
    };
    let mut bus = UdpBus::open_unsynchronized(&option, 3).unwrap();
    assert_eq!(bus.num_devices(), 3);
    let msg_id = bus.send(&frames(3, 0, Cmd::Reset)).unwrap();
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
        heartbeat: Duration::from_millis(10),
        ..emulator.option()
    };
    let mut bus = UdpBus::open(&option, 3).unwrap();
    let mut checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap();
    assert_eq!(collect(&mut bus, msg_id, 3).len(), 3);
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 3]);

    emulator.reboot(1);
    let until = Instant::now() + Duration::from_millis(250);
    while Instant::now() < until {
        let msg_id = bus.heartbeat().unwrap();
        let _ = collect_within(&mut bus, msg_id, 3, Duration::from_millis(10));
    }
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Lost, DeviceState::Lost]
    );
    for _ in 0..3 {
        let msg_id = bus.heartbeat().unwrap();
        let replies = collect_within(&mut bus, msg_id, 1, Duration::from_millis(100));
        assert_eq!(replies.iter().map(|r| r.device).collect::<Vec<_>>(), [0]);
    }

    bus.close().unwrap();
    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    drop(bus);

    let mut bus = open(&emulator, 3);
    let mut checker = bus.state_checker();
    let msg_id = bus.heartbeat().unwrap();
    assert_eq!(collect(&mut bus, msg_id, 3).len(), 3);
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 3]);
}

#[test]
fn a_host_stall_past_the_lost_timeout_reads_queued_replies_before_judging() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = emulator.option();
    let mut bus = UdpBus::open(&option, 2).unwrap();
    let mut checker = bus.state_checker();

    bus.heartbeat().unwrap();
    std::thread::sleep(option.lost_timeout * 2);
    let msg_id = bus.heartbeat().unwrap();
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);

    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );
}

#[test]
fn a_host_silent_past_the_lost_timeout_without_requests_keeps_the_devices() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = emulator.option();
    let mut bus = UdpBus::open(&option, 2).unwrap();
    let mut checker = bus.state_checker();

    std::thread::sleep(option.lost_timeout * 3);
    assert!(bus.recv(Instant::now()).unwrap().is_none());
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );

    let msg_id = bus.heartbeat().unwrap();
    assert_eq!(collect(&mut bus, msg_id, 2).len(), 2);
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );
}

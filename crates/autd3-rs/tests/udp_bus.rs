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

use std::time::Duration;

use autd3_rs::DeviceState;
use autd3_rs::protocol::{Cmd, RX_FRAME_BYTES, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::udp::{TransportOption, UdpBus, UdpError};

fn frames(n: usize, seq: u8, cmd: Cmd) -> Vec<[u8; TX_FRAME_BYTES]> {
    let mut tx = vec![[0u8; TX_FRAME_BYTES]; n];
    for buf in &mut tx {
        TxFrame::new(Seq::new(seq), cmd).write_to(buf);
    }
    tx
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
fn cycles_route_every_ack_back_to_its_device() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut bus = open(&emulator, 2);
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 2];

    let reset = frames(2, 0, Cmd::Reset);
    assert!(bus.cycle(&reset, &mut rx).unwrap().rx_valid());
    assert!(rx.iter().all(|r| r[0] == 0xFF));

    let nop = frames(2, 0, Cmd::Nop);
    assert!(bus.cycle(&nop, &mut rx).unwrap().rx_valid());
    assert!(bus.cycle(&nop, &mut rx).unwrap().rx_valid());
    assert!(rx.iter().all(|r| *r == [0, 0]));
    assert_eq!(bus.stats().lost_cycles(), 0);
    assert!(bus.stats().exchanges() >= 3);
}

#[test]
fn replies_feed_the_dc_clock() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut bus = open(&emulator, 1);
    let clock = bus.dc_clock();
    assert_eq!(clock.observation(), None);
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 1];
    for _ in 0..3 {
        bus.cycle(&frames(1, 0, Cmd::Reset), &mut rx).unwrap();
    }
    let observation = clock.observation().expect("observed");
    assert_eq!(observation.samples, 3);
    assert!(observation.offset_ns.abs() < 1_000_000_000);
}

#[test]
fn a_rebooted_middle_device_is_lost_with_everything_behind_it() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let mut bus = open(&emulator, 3);
    let mut checker = bus.state_checker();
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 3];
    let reset = frames(3, 0, Cmd::Reset);
    assert!(bus.cycle(&reset, &mut rx).unwrap().rx_valid());
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Op; 3]);

    emulator.reboot(1);
    for _ in 0..12 {
        assert!(!bus.cycle(&reset, &mut rx).unwrap().rx_valid());
    }
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Op, DeviceState::Lost, DeviceState::Lost]
    );
    assert!(bus.stats().lost_cycles() >= 12);

    bus.close().unwrap();
    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    drop(bus);

    let mut bus = open(&emulator, 3);
    let mut checker = bus.state_checker();
    assert!(bus.cycle(&reset, &mut rx).unwrap().rx_valid());
    assert_eq!(checker.check().unwrap().devices(), [DeviceState::Op; 3]);
}

use std::time::Duration;

use autd3_rs::{TransportOption, UdpBus};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{Driver, Exchange};

const SILENT_DEVICE: usize = 1;
const RESETS_LEFT_UNANSWERED: u32 = 3;

#[test]
fn reset_is_confirmed_by_a_device_that_stays_silent_past_the_lost_timeout() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = TransportOption::for_emulator(emulator.interface());
    assert!(option.lost_timeout <= Duration::from_millis(100));
    let mut bus = UdpBus::open_unsynchronized_all(&option).unwrap();
    let states = bus.state_checker();

    emulator.drop_next_frames(SILENT_DEVICE, RESETS_LEFT_UNANSWERED);
    assert!(Exchange::reset(&mut bus, Duration::from_secs(2)).unwrap());

    assert!(emulator.heartbeats_received(SILENT_DEVICE) > 0);
    assert!(!states.check().unwrap().any_lost());
    let mut driver = Driver::open(bus).unwrap();
    assert_eq!(driver.read_firmware_info().unwrap().len(), 2);
}

#[test]
fn the_handshake_survives_a_lost_reset_reply_with_the_default_lost_timeout() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = TransportOption::for_emulator(emulator.interface());
    assert_eq!(option.lost_timeout, TransportOption::default().lost_timeout);
    let bus = UdpBus::open_unsynchronized_all(&option).unwrap();
    let states = bus.state_checker();

    emulator.drop_next_replies(SILENT_DEVICE, 1);
    let mut driver = Driver::open(bus).unwrap();

    assert!(!states.check().unwrap().any_lost());
    assert_eq!(driver.read_firmware_info().unwrap().len(), 2);
}

#[test]
fn reset_reports_a_device_that_never_answers() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = TransportOption::for_emulator(emulator.interface());
    let mut bus = UdpBus::open_unsynchronized_all(&option).unwrap();

    emulator.drop_next_frames(SILENT_DEVICE, u32::MAX);
    assert!(!Exchange::reset(&mut bus, Duration::from_millis(300)).unwrap());
}

use std::time::Duration;

use autd3_rs::protocol::Cmd;
use autd3_rs::{TransportOption, UdpBus};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{Exchange, Frame, UdpLink};

const SILENT_DEVICE: usize = 1;

fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        iface: emulator.interface(),
        recovery_reply_timeout: Duration::from_millis(50),
        control_request_timeout: Duration::from_millis(50),
        enumeration_timeout: Duration::from_secs(1),
        sync_timeout: Duration::from_secs(5),
        ..TransportOption::default()
    }
}

#[test]
#[ignore = "depends on real time"]
fn an_exchange_names_a_device_that_never_answers_and_keeps_it_alive_meanwhile() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = option(&emulator);
    assert!(option.lost_timeout <= Duration::from_millis(100));
    let bus = UdpBus::open_unsynchronized_all(&option).unwrap();
    let states = bus.state_checker();
    let mut link = UdpLink::from(bus);

    emulator.drop_next_frames(SILENT_DEVICE, u32::MAX);
    let replies = link
        .exchange(&Frame::new(Cmd::Nop), Duration::from_millis(300))
        .unwrap();

    assert_eq!(replies.err(), Some(SILENT_DEVICE));
    assert!(emulator.heartbeats_received(SILENT_DEVICE) > 0);
    assert!(!states.check().unwrap().any_lost());
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reply_fails_the_exchange_without_sending_the_frame_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let bus = UdpBus::open_unsynchronized_all(&option(&emulator)).unwrap();
    let mut link = UdpLink::from(bus);

    emulator.drop_next_replies(SILENT_DEVICE, 1);
    let lost = link
        .exchange(&Frame::new(Cmd::Nop), Duration::from_millis(300))
        .unwrap();
    let answered = link
        .exchange(&Frame::new(Cmd::Nop), Duration::from_millis(300))
        .unwrap();

    assert_eq!(lost.err(), Some(SILENT_DEVICE));
    assert_eq!(answered.unwrap().len(), 2);
}

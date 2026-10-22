use std::time::Duration;

use autd3_rs::TransportOption;
use autd3_rs::protocol::Cmd;
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{DEFAULT_HEARTBEAT, Exchange, Frame, UdpLink};

const SILENT_DEVICE: usize = 1;

fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        iface: emulator.interface(),
        open_timeout: Duration::from_secs(6),
        ack_timeout: Duration::from_millis(300),
        sync_on_open: false,
        ..TransportOption::default()
    }
}

#[test]
#[ignore = "depends on real time"]
fn an_exchange_waits_for_a_device_that_never_answers_and_keeps_it_alive_meanwhile() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = option(&emulator);
    assert!(option.lost_timeout < option.ack_timeout);
    let mut link = UdpLink::open(&option, emulator.num_devices(), DEFAULT_HEARTBEAT).unwrap();

    emulator.drop_next_frames(SILENT_DEVICE, u32::MAX);
    let replies = link.exchange(&Frame::new(Cmd::Nop)).unwrap();

    assert_eq!(replies, Err(()));
    assert!(emulator.heartbeats_received(SILENT_DEVICE) > 0);
    emulator.drop_next_frames(SILENT_DEVICE, 0);
    assert_eq!(
        link.exchange(&Frame::new(Cmd::Nop)).unwrap().unwrap().len(),
        2
    );
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reply_fails_the_exchange_without_sending_the_frame_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut link = UdpLink::open(
        &option(&emulator),
        emulator.num_devices(),
        DEFAULT_HEARTBEAT,
    )
    .unwrap();

    emulator.drop_next_replies(SILENT_DEVICE, 1);
    let lost = link.exchange(&Frame::new(Cmd::Nop)).unwrap();
    let answered = link.exchange(&Frame::new(Cmd::Nop)).unwrap();

    assert_eq!(lost, Err(()));
    assert_eq!(answered.unwrap().len(), 2);
}

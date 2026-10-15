use std::time::Duration;

use autd3_cpu_wire::update::RunningImage;
use autd3_rs::protocol::Cmd;
use autd3_rs::{UdpBus, UdpOption};
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{Driver, DriverError};

const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);

fn bus_with(emulator: &UdpEmulator, option: &UdpOption) -> UdpBus {
    UdpBus::open_unsynchronized(option, emulator.num_devices()).unwrap()
}

fn bus(emulator: &UdpEmulator) -> UdpBus {
    bus_with(emulator, &UdpOption::for_emulator(emulator.interface()))
}

fn open(emulator: &UdpEmulator) -> Driver<UdpBus> {
    Driver::open(bus(emulator)).unwrap()
}

#[test]
#[ignore = "depends on real time"]
fn the_handshake_and_the_reads_go_through_the_udp_bus() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    assert_eq!(driver.num_devices(), 2);
    let infos = driver.read_firmware_info().unwrap();
    assert_eq!(infos.len(), 2);
    for info in infos {
        assert_eq!(
            info.cpu_version,
            [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
        );
    }
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
    driver.close().unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_frame_is_retransmitted() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_frames(1, 1);

    assert_eq!(driver.read_firmware_info().unwrap().len(), 2);
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reply_is_recovered() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_replies(0, 1);
    driver.confirm().unwrap();

    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reset_reply_is_recovered_when_the_lost_timeout_outlasts_the_retransmission() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = UdpOption {
        lost_timeout: Duration::from_secs(1),
        ..UdpOption::for_emulator(emulator.interface())
    };
    let bus = bus_with(&emulator, &option);

    emulator.drop_next_replies(1, 1);
    let mut driver = Driver::open(bus).unwrap();

    assert_eq!(driver.read_firmware_info().unwrap().len(), 2);
}

#[test]
#[ignore = "depends on real time"]
fn a_handshake_nobody_answers_is_reported_as_an_unconfirmed_reset() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let bus = bus(&emulator);

    emulator.set_muted(1, true);

    assert!(matches!(
        Driver::open(bus),
        Err(DriverError::ResetUnconfirmed)
    ));
}

#[test]
#[ignore = "depends on real time"]
fn a_device_that_stops_answering_is_named_in_the_timeout() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.set_muted(1, true);
    let error = driver.read_firmware_info().unwrap_err();

    assert!(matches!(
        error,
        DriverError::Timeout {
            device: 1,
            cmd: Cmd::ReadFirmwareInfo,
            ..
        }
    ));
}

#[test]
#[ignore = "depends on real time"]
fn a_closed_bus_fails_the_next_request_with_a_network_error() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let bus = bus(&emulator);
    bus.close().unwrap();

    assert!(matches!(Driver::open(bus), Err(DriverError::Network(_))));
}

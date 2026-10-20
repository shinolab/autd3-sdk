use std::time::Duration;

use autd3_cpu_wire::update::RunningImage;
use autd3_rs::protocol::Cmd;
use autd3_rs::{TransportOption, UdpBus};
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{Driver, DriverError, UdpLink};

const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);

fn bus_with(emulator: &UdpEmulator, option: &TransportOption) -> UdpBus {
    UdpBus::open_unsynchronized(option, emulator.num_devices()).unwrap()
}

fn bus(emulator: &UdpEmulator) -> UdpBus {
    let option = TransportOption {
        iface: emulator.interface(),
        recovery_reply_timeout: Duration::from_millis(50),
        control_request_timeout: Duration::from_millis(50),
        enumeration_timeout: Duration::from_secs(1),
        sync_timeout: Duration::from_secs(5),
        ..TransportOption::default()
    };
    bus_with(emulator, &option)
}

fn open(emulator: &UdpEmulator) -> Driver<UdpLink> {
    Driver::new(UdpLink::from(bus(emulator)))
}

#[test]
#[ignore = "depends on real time"]
fn the_reads_go_through_the_udp_bus() {
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
fn a_lost_frame_fails_the_request_without_sending_it_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_frames(1, 1);

    assert!(matches!(
        driver.read_firmware_info(),
        Err(DriverError::Timeout {
            device: 1,
            cmd: Cmd::ReadFirmwareInfo,
            ..
        })
    ));
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reply_fails_the_request_although_the_device_ran_it() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_replies(0, 1);
    assert!(matches!(
        driver.confirm(),
        Err(DriverError::Timeout {
            device: 0,
            cmd: Cmd::UpdateConfirm,
            ..
        })
    ));

    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
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

    assert!(matches!(
        Driver::new(UdpLink::from(bus)).read_firmware_info(),
        Err(DriverError::Network(_))
    ));
}

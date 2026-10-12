use autd3_cpu_wire::update::RunningImage;
use autd3_rs::{TransportOption, UdpBus};
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{Driver, DriverError};

const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);

fn bus(emulator: &UdpEmulator) -> UdpBus {
    UdpBus::open_unsynchronized(
        &TransportOption::for_emulator(emulator.interface()),
        emulator.num_devices(),
    )
    .unwrap()
}

fn open(emulator: &UdpEmulator) -> Driver<UdpBus> {
    Driver::open(bus(emulator)).unwrap()
}

#[test]
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
fn a_closed_bus_fails_the_next_request_with_a_network_error() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let bus = bus(&emulator);
    bus.close().unwrap();

    assert!(matches!(Driver::open(bus), Err(DriverError::Network(_))));
}

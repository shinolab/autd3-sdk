mod common;

use std::time::Duration;

use autd3_rs::commands::Nop;
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::{Client, Error, FirmwareVersion};

use common::{emulator, geometry, open_with, option, udp_error};

fn open_error(num_devices: usize, option: &TransportOption) -> Error {
    Client::open(&geometry(num_devices), option).err().unwrap()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_empty_geometry_is_rejected() {
    let emulator = emulator(1);

    let error = open_error(0, &option(&emulator));

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::InvalidDeviceCount(0))
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_geometry_with_more_devices_than_attached_times_out_the_enumeration() {
    let emulator = emulator(1);
    let option = TransportOption {
        open_timeout: Duration::from_millis(200),
        ..option(&emulator)
    };

    let error = open_error(2, &option);

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::EnumerationTimeout {
            expected: 2,
            found: 1
        })
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_geometry_with_fewer_devices_than_attached_is_a_count_mismatch() {
    let emulator = emulator(2);

    let error = open_error(1, &option(&emulator));

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::DeviceCountMismatch {
            expected: 1,
            found: 2
        })
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_bundled_firmware_is_in_the_supported_series() {
    let emulator = emulator(2);
    let client = open_with(&emulator, &option(&emulator));

    let versions = client.read_firmware_version().await.unwrap();

    assert_eq!(versions.len(), 2);
    assert!(versions.iter().all(FirmwareVersion::is_supported));
    client.close().unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_devices_can_be_opened_again_after_a_close() {
    let emulator = emulator(3);
    for _ in 0..2 {
        let client = open_with(&emulator, &option(&emulator));
        client.send(Nop).await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 3);
        client.close().unwrap();
    }
}

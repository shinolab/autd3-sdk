mod common;

use autd3_rs::commands::Nop;
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::{Controller, Error, FirmwareVersion};

use common::{emulator, geometry, open_with, option, udp_error};

async fn open_error(num_devices: usize, option: &TransportOption) -> Error {
    Controller::open(&geometry(num_devices), option)
        .await
        .err()
        .unwrap()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_empty_geometry_is_rejected() {
    let emulator = emulator(1);

    let error = open_error(0, &option(&emulator)).await;

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::InvalidDeviceCount(0))
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_geometry_with_fewer_devices_than_attached_is_a_count_mismatch() {
    let emulator = emulator(2);

    let error = open_error(1, &option(&emulator)).await;

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
    let client = open_with(&emulator, &option(&emulator)).await;

    let versions = client.read_firmware_version().await.unwrap();

    assert_eq!(versions.len(), 2);
    assert!(versions.iter().all(FirmwareVersion::is_supported));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_devices_can_be_opened_again_after_a_drop() {
    let emulator = emulator(3);
    for _ in 0..2 {
        let client = open_with(&emulator, &option(&emulator)).await;
        client.send(Nop).await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 3);
    }
}

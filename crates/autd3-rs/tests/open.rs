mod common;

use std::time::Duration;

use autd3_rs::commands::{Clear, ForceFan, Nop};
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::{Client, Error, FirmwareVersion, MAX_DEVICES, PayloadError};

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
        error,
        Error::InvalidPayload(PayloadError::DeviceCountOutOfRange {
            got: 0,
            max: MAX_DEVICES
        })
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_zero_ack_timeout_is_rejected() {
    let emulator = emulator(1);
    let option = TransportOption {
        ack_timeout: Duration::ZERO,
        ..option(&emulator)
    };

    let error = open_error(1, &option);

    assert!(matches!(udp_error(&error), Some(UdpError::ZeroAckTimeout)));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_invalid_transport_option_is_rejected() {
    let emulator = emulator(1);
    let option = TransportOption {
        recovery_reply_timeout: Duration::ZERO,
        ..option(&emulator)
    };

    let error = open_error(1, &option);

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::ZeroRecoveryReplyTimeout)
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_geometry_with_more_devices_than_attached_times_out_the_enumeration() {
    let emulator = emulator(1);
    let option = TransportOption {
        enumeration_timeout: Duration::from_millis(200),
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
async fn open_fails_when_a_device_never_locks_to_the_grandmaster() {
    let emulator = emulator(2);
    emulator.set_ptp_lock_blocked(1, true);
    let option = TransportOption {
        sync_timeout: Duration::from_millis(200),
        ..option(&emulator)
    };

    let error = open_error(2, &option);

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::SyncTimeout { not_ready, .. }) if not_ready == &[1]
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
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn open_keeps_what_a_previous_session_left_until_the_user_clears_it() {
    let emulator = emulator(2);
    let first = open_with(&emulator, &option(&emulator));
    first.send(ForceFan { value: true }).await.unwrap();
    drop(first);

    let second = open_with(&emulator, &option(&emulator));

    assert!(emulator.with_devices(|devices| devices.iter().all(|d| d.fpga().force_fan())));
    assert_eq!(second.bus_stats().frames(), 0);
    second.send(Clear).await.unwrap();
    assert!(emulator.with_devices(|devices| devices.iter().all(|d| !d.fpga().force_fan())));
    second.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_devices_can_be_opened_again_after_a_close() {
    let emulator = emulator(3);
    for _ in 0..2 {
        let client = open_with(&emulator, &option(&emulator));
        client.send(Nop).await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 3);
        client.close().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn open_survives_a_lost_unit_info_reply() {
    let emulator = emulator(2);
    emulator.drop_next_unit_infos(0, 1);
    emulator.drop_next_unit_infos(1, 1);

    let client = open_with(&emulator, &option(&emulator));

    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

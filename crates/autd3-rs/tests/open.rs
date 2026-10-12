mod common;

use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::{Client, ClientConfig, Error, MAX_DEVICES, MAX_INFLIGHT, PayloadError};

use common::{config, emulator, geometry, open_with, option, udp_error};

async fn open_error(num_devices: usize, option: &TransportOption, config: ClientConfig) -> Error {
    Client::open(&geometry(num_devices), option, config)
        .await
        .err()
        .unwrap()
}

#[tokio::test]
async fn an_empty_geometry_is_rejected() {
    let emulator = emulator(1);

    let error = open_error(0, &option(&emulator), config()).await;

    assert!(matches!(
        error,
        Error::InvalidPayload(PayloadError::DeviceCountOutOfRange {
            got: 0,
            max: MAX_DEVICES
        })
    ));
}

#[tokio::test]
async fn a_window_wider_than_the_device_queue_is_rejected() {
    let emulator = emulator(1);
    let config = ClientConfig {
        max_inflight: NonZeroUsize::new(MAX_INFLIGHT + 1).unwrap(),
        ..config()
    };

    let error = open_error(1, &option(&emulator), config).await;

    assert!(matches!(
        error,
        Error::InvalidPayload(PayloadError::MaxInflightTooLarge { max: MAX_INFLIGHT })
    ));
}

#[tokio::test]
async fn a_zero_ack_timeout_is_rejected() {
    let emulator = emulator(1);
    let config = ClientConfig {
        ack_timeout: Duration::ZERO,
        ..config()
    };

    let error = open_error(1, &option(&emulator), config).await;

    assert!(matches!(
        error,
        Error::InvalidPayload(PayloadError::ZeroAckTimeout)
    ));
}

#[tokio::test]
async fn an_invalid_transport_option_is_rejected() {
    let emulator = emulator(1);
    let option = TransportOption {
        reply_timeout: Duration::ZERO,
        ..option(&emulator)
    };

    let error = open_error(1, &option, config()).await;

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::InvalidOption {
            field: "reply_timeout",
            ..
        })
    ));
}

#[tokio::test]
async fn a_geometry_with_fewer_devices_than_attached_is_a_count_mismatch() {
    let emulator = emulator(2);

    let error = open_error(1, &option(&emulator), config()).await;

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::DeviceCountMismatch {
            expected: 1,
            found: 2
        })
    ));
}

#[tokio::test]
async fn the_bundled_firmware_passes_the_supported_series_check() {
    let emulator = emulator(2);
    let config = ClientConfig {
        require_supported_firmware: true,
        ..config()
    };

    let client = open_with(&emulator, &option(&emulator), config).await;

    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn open_clears_what_a_previous_session_left_on_the_devices() {
    let emulator = emulator(2);
    let first = open_with(&emulator, &option(&emulator), config()).await;
    first.send(ForceFan { value: true }).await.unwrap();
    drop(first);
    assert!(emulator.with_devices(|devices| devices.iter().all(|d| d.fpga().force_fan())));

    let second = open_with(&emulator, &option(&emulator), config()).await;

    assert!(emulator.with_devices(|devices| devices.iter().all(|d| !d.fpga().force_fan())));
    assert_eq!(second.bus_stats().resets(), 1);
    second.send(Nop).await.unwrap();
    second.close().await.unwrap();
}

#[tokio::test]
async fn the_devices_can_be_opened_again_after_a_close() {
    let emulator = emulator(3);
    for _ in 0..2 {
        let client = open_with(&emulator, &option(&emulator), config()).await;
        client.send(Nop).await.unwrap();
        assert_eq!(client.read_firmware_version().await.unwrap().len(), 3);
        client.close().await.unwrap();
    }
}

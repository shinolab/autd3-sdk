mod common;

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::udp::{UdpError, UdpOption};
use autd3_rs::{
    Client, ClientConfig, ClientConfigError, Error, MAX_DEVICES, MAX_INFLIGHT, PayloadError,
};

use common::{config, emulator, geometry, open_with, option, udp_error};

async fn open_error(num_devices: usize, option: &UdpOption, config: ClientConfig) -> Error {
    Client::open(&geometry(num_devices), option, config)
        .await
        .err()
        .unwrap()
}

#[tokio::test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
async fn a_window_wider_than_the_device_queue_is_rejected() {
    let emulator = emulator(1);
    let config = ClientConfig {
        max_inflight: NonZeroUsize::new(MAX_INFLIGHT + 1).unwrap(),
        ..config()
    };

    let error = open_error(1, &option(&emulator), config).await;

    assert!(matches!(
        error,
        Error::InvalidConfig(ClientConfigError::MaxInflightTooLarge { max: MAX_INFLIGHT })
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_zero_ack_timeout_is_rejected() {
    let emulator = emulator(1);
    let config = ClientConfig {
        ack_timeout: Duration::ZERO,
        ..config()
    };

    let error = open_error(1, &option(&emulator), config).await;

    assert!(matches!(
        error,
        Error::InvalidConfig(ClientConfigError::ZeroAckTimeout)
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_exec_lead_not_below_lost_timeout_is_rejected() {
    let emulator = emulator(1);
    let option = option(&emulator);
    let config = ClientConfig {
        exec_lead: Some(option.lost_timeout),
        ..config()
    };

    let error = open_error(1, &option, config).await;

    assert!(matches!(
        error,
        Error::InvalidConfig(ClientConfigError::ExecLeadNotBelowLostTimeout { .. })
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_invalid_udp_option_is_rejected() {
    let emulator = emulator(1);
    let option = UdpOption {
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
#[ignore = "depends on real time"]
async fn a_geometry_with_more_devices_than_attached_times_out_the_enumeration() {
    let emulator = emulator(1);
    let option = UdpOption {
        enumeration_timeout: Duration::from_millis(200),
        ..option(&emulator)
    };

    let error = open_error(2, &option, config()).await;

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
#[ignore = "depends on real time"]
async fn open_fails_when_a_device_never_locks_to_the_grandmaster() {
    let emulator = emulator(2);
    emulator.set_ptp_lock_blocked(1, true);
    let option = UdpOption {
        sync_timeout: Duration::from_millis(200),
        ..option(&emulator)
    };

    let error = open_error(2, &option, config()).await;

    assert!(matches!(
        udp_error(&error),
        Some(UdpError::SyncTimeout { not_ready, .. }) if not_ready == &[1]
    ));
}

#[tokio::test]
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
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
#[ignore = "depends on real time"]
async fn the_devices_can_be_opened_again_after_a_close() {
    let emulator = emulator(3);
    for _ in 0..2 {
        let client = open_with(&emulator, &option(&emulator), config()).await;
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

    let client = open_with(&emulator, &option(&emulator), config()).await;

    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn open_sends_the_reset_again_when_its_reply_is_lost() {
    let emulator = emulator(2);
    emulator.drop_next_replies(1, 1);

    let client = open_with(&emulator, &option(&emulator), config()).await;

    assert_eq!(client.bus_stats().resets(), 2);
    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn open_times_out_when_the_reset_is_never_acknowledged() {
    let emulator = emulator(1);
    emulator.drop_next_replies(0, 2);
    let config = ClientConfig {
        max_resync_rounds: NonZeroU32::new(2).unwrap(),
        ..config()
    };

    let error = open_error(1, &option(&emulator), config).await;

    assert!(matches!(error, Error::Timeout { .. }));
    let client = open_with(&emulator, &option(&emulator), common::config()).await;
    client.close().await.unwrap();
}

#![allow(dead_code)]

use std::time::Duration;

use autd3_rs::commands::ActivateModulationBank;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::{UdpError, UdpOption};
use autd3_rs::value::{ModulationBank, TransitionMode};
use autd3_rs::{Client, ClientConfig, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub(crate) const ACK_TIMEOUT: Duration = Duration::from_millis(250);
pub(crate) const LONG_ACK_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const LOST_TIMEOUT: Duration = Duration::from_millis(500);
pub(crate) const PATIENCE: Duration = Duration::from_secs(10);
pub(crate) const INVALID_TRANSITION_MODE: u8 = 0x05;

pub(crate) const REJECTED_BY_THE_DEVICE: ActivateModulationBank = ActivateModulationBank {
    bank: ModulationBank::B0,
    transition_mode: TransitionMode::SyncIdx,
};

pub(crate) fn geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()))
}

pub(crate) fn emulator(num_devices: usize) -> UdpEmulator {
    UdpEmulator::spawn(num_devices).unwrap()
}

pub(crate) fn option(emulator: &UdpEmulator) -> UdpOption {
    UdpOption {
        iface: emulator.interface(),
        heartbeat: None,
        lost_timeout: PATIENCE,
        reply_timeout: Duration::from_millis(50),
        response_timeout: Duration::from_millis(50),
        enumeration_timeout: Duration::from_secs(1),
        sync_timeout: Duration::from_secs(5),
        ..UdpOption::default()
    }
}

pub(crate) fn monitoring_option(emulator: &UdpEmulator) -> UdpOption {
    UdpOption {
        heartbeat: Some(Duration::from_millis(10)),
        lost_timeout: LOST_TIMEOUT,
        ..option(emulator)
    }
}

pub(crate) fn config() -> ClientConfig {
    ClientConfig {
        ack_timeout: ACK_TIMEOUT,
        ..ClientConfig::default()
    }
}

pub(crate) fn patient_config() -> ClientConfig {
    ClientConfig {
        ack_timeout: LONG_ACK_TIMEOUT,
        ..ClientConfig::default()
    }
}

pub(crate) async fn open_with(
    emulator: &UdpEmulator,
    option: &UdpOption,
    config: ClientConfig,
) -> Client {
    Client::open(&geometry(emulator.num_devices()), option, config)
        .await
        .unwrap()
}

pub(crate) async fn open(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &option(&emulator), config()).await;
    (emulator, client)
}

pub(crate) fn udp_error(error: &Error) -> Option<&UdpError> {
    match error {
        Error::Network(cause) => (**cause).downcast_ref::<UdpError>(),
        Error::DeviceError { .. }
        | Error::UnsupportedFirmware { .. }
        | Error::UnexpectedReply { .. }
        | Error::DeviceLost { .. }
        | Error::Timeout { .. }
        | Error::SeqMismatch { .. }
        | Error::DeviceTimeUnknown
        | Error::InvalidPayload(_)
        | Error::InvalidConfig(_)
        | Error::Encode(_)
        | Error::Closed => None,
        _ => unreachable!(),
    }
}

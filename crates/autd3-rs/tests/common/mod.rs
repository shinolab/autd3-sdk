#![allow(dead_code)]

use std::time::Duration;

use autd3_rs::commands::ActivateModulationBank;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::value::{ModulationBank, TransitionMode};
use autd3_rs::{Client, ClientConfig, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub const ACK_TIMEOUT: Duration = Duration::from_millis(250);
pub const LONG_ACK_TIMEOUT: Duration = Duration::from_secs(30);
pub const LOST_TIMEOUT: Duration = Duration::from_millis(500);
pub const PATIENCE: Duration = Duration::from_secs(10);
pub const INVALID_TRANSITION_MODE: u8 = 0x05;

pub const REJECTED_BY_THE_DEVICE: ActivateModulationBank = ActivateModulationBank {
    bank: ModulationBank::B0,
    transition_mode: TransitionMode::SyncIdx,
};

pub fn geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()).collect())
}

pub fn emulator(num_devices: usize) -> UdpEmulator {
    UdpEmulator::spawn(num_devices).unwrap()
}

pub fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        heartbeat: None,
        lost_timeout: PATIENCE,
        ..TransportOption::for_emulator(emulator.interface())
    }
}

pub fn monitoring_option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        heartbeat: Some(Duration::from_millis(10)),
        lost_timeout: LOST_TIMEOUT,
        ..option(emulator)
    }
}

pub fn config() -> ClientConfig {
    ClientConfig {
        ack_timeout: ACK_TIMEOUT,
        ..ClientConfig::default()
    }
}

pub fn patient_config() -> ClientConfig {
    ClientConfig {
        ack_timeout: LONG_ACK_TIMEOUT,
        ..ClientConfig::default()
    }
}

pub async fn open_with(
    emulator: &UdpEmulator,
    option: &TransportOption,
    config: ClientConfig,
) -> Client {
    Client::open(&geometry(emulator.num_devices()), option, config)
        .await
        .unwrap()
}

pub async fn open(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &option(&emulator), config()).await;
    (emulator, client)
}

pub fn udp_error(error: &Error) -> Option<&UdpError> {
    match error {
        Error::Network(cause) => (**cause).downcast_ref::<UdpError>(),
        _ => None,
    }
}

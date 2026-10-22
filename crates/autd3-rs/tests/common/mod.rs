#![allow(dead_code)]

use std::time::Duration;

use autd3_rs::commands::ActivateModulationBank;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::value::{ModulationBank, TransitionMode};
use autd3_rs::{Client, Error};
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

pub(crate) fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        iface: emulator.interface(),
        lost_timeout: PATIENCE,
        open_timeout: Duration::from_secs(6),
        ack_timeout: ACK_TIMEOUT,
        ..TransportOption::default()
    }
}

pub(crate) fn monitoring_option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        lost_timeout: LOST_TIMEOUT,
        ..option(emulator)
    }
}

pub(crate) fn patient_option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        ack_timeout: LONG_ACK_TIMEOUT,
        ..option(emulator)
    }
}

pub(crate) fn open_with(emulator: &UdpEmulator, option: &TransportOption) -> Client {
    Client::open(&geometry(emulator.num_devices()), option).unwrap()
}

pub(crate) fn open(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &option(&emulator));
    (emulator, client)
}

pub(crate) fn open_monitored(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &monitoring_option(&emulator));
    (emulator, client)
}

pub(crate) fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

pub(crate) fn udp_error(error: &Error) -> Option<&UdpError> {
    match error {
        Error::Network(cause) => (**cause).downcast_ref::<UdpError>(),
        Error::DeviceError { .. }
        | Error::UnexpectedReply { .. }
        | Error::DeviceLost { .. }
        | Error::Timeout { .. }
        | Error::DeviceTimeUnknown
        | Error::InvalidPayload(_)
        | Error::Closed => None,
        _ => unreachable!(),
    }
}

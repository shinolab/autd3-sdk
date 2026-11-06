#![allow(dead_code)]

use std::time::Duration;

use autd3_rs::commands::{CpuConfig, PtpConfig, SetCpuConfig};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::{TransportOption, UdpError};
use autd3_rs::{Client, Error, ExecAt, Frame, Response};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub(crate) const PATIENCE: Duration = Duration::from_secs(10);
pub(crate) const INVALID_PAYLOAD: u8 = 0x02;

pub(crate) const REJECTED_BY_THE_DEVICE: SetCpuConfig = SetCpuConfig::new(CpuConfig {
    ptp: PtpConfig {
        sync_interval: Duration::ZERO,
        ..PtpConfig::DEFAULT
    },
    ..CpuConfig::DEFAULT
});

pub(crate) fn geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()))
}

pub(crate) fn emulator(num_devices: usize) -> UdpEmulator {
    UdpEmulator::spawn(num_devices).unwrap()
}

pub(crate) fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        iface: emulator.interface(),
        open_timeout: Duration::from_secs(6),
        ..TransportOption::default()
    }
}

pub(crate) async fn open_with(emulator: &UdpEmulator, option: &TransportOption) -> Client {
    Client::open(&geometry(emulator.num_devices()), option)
        .await
        .unwrap()
}

pub(crate) async fn open(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &option(&emulator)).await;
    (emulator, client)
}

pub(crate) async fn send_and_collect(client: &Client, frame: Frame<'_>, exec: ExecAt) -> Response {
    let msg_id = client.send_frame(frame, exec).await.unwrap();
    tokio::time::timeout(PATIENCE, client.collect(msg_id))
        .await
        .unwrap()
        .unwrap()
}

pub(crate) fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

pub(crate) fn udp_error(error: &Error) -> Option<&UdpError> {
    match error {
        Error::Network(cause) => Some(cause),
        Error::DeviceError { .. }
        | Error::UnexpectedReply { .. }
        | Error::DeviceLost { .. }
        | Error::DeviceTimeUnknown
        | Error::InvalidPayload(_) => None,
        _ => unreachable!(),
    }
}

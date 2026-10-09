#![allow(dead_code)]

use autd3_rs::geometry::{Autd3, Geometry};
use std::time::Duration;

use autd3_rs::udp::TransportOption;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

pub fn geometry(n: usize) -> Geometry {
    Geometry::new((0..n).map(|_| Autd3::default()).collect())
}

pub async fn open(emulator: &UdpEmulator) -> Client {
    open_with(emulator, ClientConfig::default()).await
}

pub async fn open_with(emulator: &UdpEmulator, config: ClientConfig) -> Client {
    Client::open(&geometry(emulator.num_devices()), &option(emulator), config)
        .await
        .unwrap()
}

pub fn full_modulation() -> Vec<u8> {
    (0..autd3_rs::params::MOD_BUFFER_SAMPLES)
        .map(|i| u8::try_from((i * 7) % 251).unwrap())
        .collect()
}

pub fn option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        iface: emulator.interface(),
        reply_timeout: Duration::from_millis(50),
        response_timeout: Duration::from_millis(50),
        enumeration_timeout: Duration::from_secs(1),
        sync_timeout: Duration::from_secs(5),
        ..TransportOption::default()
    }
}

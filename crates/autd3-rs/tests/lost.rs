mod common;

use autd3_rs::{Client, DeviceState};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{emulator, monitoring_option, open_with};

const READY: DeviceState = DeviceState::Ready;

fn open_monitored(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &monitoring_option(&emulator));
    (emulator, client)
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_device_is_ready_after_open() {
    let (_emulator, client) = open_monitored(3);

    let status = client.state_checker().check().unwrap();

    assert_eq!(status.devices(), [READY; 3]);
    assert!(status.all_ready());
    assert!(!status.any_lost());
    client.close().await.unwrap();
}

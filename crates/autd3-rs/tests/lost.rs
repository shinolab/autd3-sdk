mod common;

use autd3_rs::{Client, DeviceState};

use common::open_monitored;

const READY: DeviceState = DeviceState::Ready;

fn states(client: &Client) -> Vec<DeviceState> {
    client
        .device_status()
        .unwrap()
        .iter()
        .map(|status| status.state)
        .collect()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_device_is_ready_after_open() {
    let (_emulator, client) = open_monitored(3);

    assert_eq!(states(&client), [READY; 3]);
    client.close().unwrap();
}

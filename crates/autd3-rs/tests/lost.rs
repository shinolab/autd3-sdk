mod common;

use autd3_rs::commands::Nop;
use autd3_rs::{DeviceState, ExecAt, Frames};

use common::{open, send_and_collect};

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_device_is_ready_after_open() {
    let (_emulator, client) = open(3).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();

    let response = send_and_collect(&client, nop.frame(0).unwrap(), ExecAt::Immediate).await;

    assert!(
        response
            .device_status()
            .iter()
            .all(|status| status.state == DeviceState::Ready)
    );
}

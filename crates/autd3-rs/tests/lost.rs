mod common;

use autd3_rs::DeviceState;

use common::open_monitored;

const READY: DeviceState = DeviceState::Ready;

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

mod common;

use std::time::{Duration, Instant};

use autd3_rs::{Client, DeviceState};

use common::{PATIENCE, open_monitored};

const READY: DeviceState = DeviceState::Ready;
const SETTLE: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

fn states(client: &Client) -> Vec<DeviceState> {
    client.state_checker().check().unwrap().devices().to_vec()
}

async fn wait_until_lost(client: &Client, device: usize) {
    let give_up = Instant::now() + PATIENCE;
    while states(client)[device] != DeviceState::Lost {
        assert!(
            Instant::now() < give_up,
            "the device did not become lost: {:?}",
            states(client)
        );
        client.send_heartbeat().unwrap();
        tokio::time::sleep(POLL_INTERVAL).await;
    }
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

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_device_that_answers_again_stays_lost() {
    let (emulator, client) = open_monitored(2);

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    emulator.set_muted(1, false);
    client.send_heartbeat().unwrap();
    tokio::time::sleep(SETTLE).await;

    assert_eq!(states(&client)[1], DeviceState::Lost);
    client.close().await.unwrap();
}

mod common;

use std::time::{Duration, Instant};

use autd3_rs::commands::Nop;
use autd3_rs::{DeviceState, Error};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{PATIENCE, open};

fn heartbeats(emulator: &UdpEmulator) -> Vec<u64> {
    (0..emulator.num_devices())
        .map(|device| emulator.heartbeats_received(device))
        .collect()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_client_sends_no_heartbeat_by_itself() {
    let (emulator, client) = open(2).await;

    for _ in 0..3 {
        client.send(Nop).await.unwrap();
    }

    assert_eq!(heartbeats(&emulator), [0, 0]);
    assert_eq!(client.bus_stats().heartbeats(), 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn each_heartbeat_reaches_every_device_once() {
    let (emulator, client) = open(3).await;

    for _ in 0..5 {
        client.heartbeat().unwrap();
    }
    client.send(Nop).await.unwrap();

    assert_eq!(heartbeats(&emulator), [5, 5, 5]);
    assert_eq!(client.bus_stats().heartbeats(), 5);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_heartbeat_reply_updates_the_device_state() {
    let (emulator, client) = open(2).await;
    let checker = client.state_checker();

    emulator.set_ptp_lock_blocked(1, true);
    assert_eq!(
        checker.check().unwrap().devices(),
        [DeviceState::Ready, DeviceState::Ready]
    );

    client.heartbeat().unwrap();
    let give_up = Instant::now() + PATIENCE;
    while checker.check().unwrap().devices()[1] != DeviceState::Syncing {
        assert!(
            Instant::now() < give_up,
            "the heartbeat reply never arrived"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    assert_eq!(checker.check().unwrap().devices()[0], DeviceState::Ready);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_heartbeat_after_close_is_refused() {
    let (emulator, client) = open(1).await;
    client.close().await.unwrap();

    assert!(matches!(client.heartbeat(), Err(Error::Closed)));
    assert_eq!(heartbeats(&emulator), [0]);
}

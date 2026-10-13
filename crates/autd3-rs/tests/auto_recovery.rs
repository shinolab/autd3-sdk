mod common;

use std::time::{Duration, Instant};

use autd3_rs::commands::{ForceFan, Nop};
use autd3_rs::udp::TransportOption;
use autd3_rs::{Client, ClientConfig, DeviceState};
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{PATIENCE, config, emulator, monitoring_option, open_with};

const ADDR_SYNC_TIME_0: u16 = 0x10;
const RECOVERY_INTERVAL: Duration = Duration::from_millis(20);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

fn recovering_option(emulator: &UdpEmulator) -> TransportOption {
    TransportOption {
        recovery_interval: Some(RECOVERY_INTERVAL),
        ..monitoring_option(emulator)
    }
}

async fn open_recovering(num_devices: usize, config: ClientConfig) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &recovering_option(&emulator), config).await;
    (emulator, client)
}

fn states(client: &Client) -> Vec<DeviceState> {
    client.state_checker().check().unwrap().devices().to_vec()
}

async fn wait_until(client: &Client, what: &str, reached: impl Fn(&[DeviceState]) -> bool) {
    let give_up = Instant::now() + PATIENCE;
    while !reached(&states(client)) {
        assert!(
            Instant::now() < give_up,
            "{what} was not reached: {:?}",
            states(client)
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_until_lost(client: &Client, device: usize) {
    wait_until(client, "a lost device", |states| {
        states[device] == DeviceState::Lost
    })
    .await;
}

async fn wait_until_all_ready(client: &Client) {
    wait_until(client, "every device ready", |states| {
        states.iter().all(|&state| state == DeviceState::Ready)
    })
    .await;
}

fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

fn failsafes(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().failsafe()).collect())
}

async fn wait_until_failsafe(emulator: &UdpEmulator, device: usize) {
    let give_up = Instant::now() + PATIENCE;
    while !failsafes(emulator)[device] {
        assert!(Instant::now() < give_up, "the failsafe did not trip");
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn recover_from_failsafe(emulator: &UdpEmulator, client: &Client) {
    emulator.set_muted(1, true);
    wait_until_lost(client, 1).await;
    wait_until_failsafe(emulator, 1).await;
    emulator.set_muted(1, false);
    wait_until_all_ready(client).await;
    client.send(Nop).await.unwrap();
}

fn sync_times(emulator: &UdpEmulator) -> Vec<Vec<u16>> {
    emulator.with_devices(|devices| {
        devices
            .iter()
            .map(|d| {
                (0..4)
                    .map(|i| d.fpga().controller_reg(ADDR_SYNC_TIME_0 + i))
                    .collect()
            })
            .collect()
    })
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn without_a_recovery_interval_a_device_that_answers_again_stays_lost() {
    let emulator = emulator(2);
    let client = open_with(&emulator, &monitoring_option(&emulator), config()).await;

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    emulator.set_muted(1, false);
    tokio::time::sleep(RECOVERY_INTERVAL * 10).await;

    assert_eq!(states(&client)[1], DeviceState::Lost);
    assert_eq!(client.bus_stats().recoveries(), 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_device_that_answers_again_is_recovered_and_takes_frames() {
    let (emulator, client) = open_recovering(3, config()).await;

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    client.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true, false, true]);
    emulator.set_muted(1, false);
    wait_until_all_ready(&client).await;

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(fans(&emulator), [true, true, true]);
    assert_eq!(client.bus_stats().recoveries(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rebooted_device_stays_lost() {
    let (emulator, client) = open_recovering(3, config()).await;

    emulator.reboot(1);
    wait_until_lost(&client, 1).await;
    tokio::time::sleep(RECOVERY_INTERVAL * 10).await;

    assert_eq!(states(&client)[1], DeviceState::Lost);
    assert_eq!(client.bus_stats().recoveries(), 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_device_cut_off_at_once_is_recovered() {
    let (emulator, client) = open_recovering(3, config()).await;

    for device in 0..3 {
        emulator.set_muted(device, true);
    }
    wait_until(&client, "every device lost", |states| {
        states.iter().all(|&state| state == DeviceState::Lost)
    })
    .await;
    for device in 0..3 {
        emulator.set_muted(device, false);
    }
    wait_until_all_ready(&client).await;

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(fans(&emulator), [true, true, true]);
    assert_eq!(client.bus_stats().recoveries(), 3);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn only_the_recovered_device_is_synchronized_again() {
    let (emulator, client) = open_recovering(3, config()).await;
    let before = sync_times(&emulator);

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    emulator.set_muted(1, false);
    wait_until_all_ready(&client).await;

    client.send(Nop).await.unwrap();

    let after = sync_times(&emulator);
    assert_eq!(after[0], before[0]);
    assert_ne!(after[1], before[1]);
    assert_eq!(after[2], before[2]);

    client.send(Nop).await.unwrap();

    assert_eq!(sync_times(&emulator), after);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_recovered_device_is_not_synchronized_again_when_turned_off() {
    let config = ClientConfig {
        resynchronize_on_recovery: None,
        ..config()
    };
    let (emulator, client) = open_recovering(2, config).await;
    let before = sync_times(&emulator);

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    emulator.set_muted(1, false);
    wait_until_all_ready(&client).await;

    client.send(Nop).await.unwrap();

    assert_eq!(sync_times(&emulator), before);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_failsafe_of_a_recovered_device_is_released() {
    let (emulator, client) = open_recovering(3, config()).await;

    recover_from_failsafe(&emulator, &client).await;

    assert_eq!(failsafes(&emulator), [false, false, false]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_failsafe_of_a_recovered_device_is_kept_when_turned_off() {
    let config = ClientConfig {
        release_failsafe_on_recovery: None,
        ..config()
    };
    let (emulator, client) = open_recovering(3, config).await;

    recover_from_failsafe(&emulator, &client).await;

    assert_eq!(failsafes(&emulator), [false, true, false]);
    client.close().await.unwrap();
}

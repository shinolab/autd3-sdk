mod common;

use std::time::{Duration, Instant};

use autd3_rs::commands::{
    CpuConfig, ForceFan, Nop, ReleaseFailsafe, SetCpuConfig, Synchronize, each,
};
use autd3_rs::{Client, DeviceMask, DeviceState};
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{PATIENCE, config, emulator, monitoring_option, open_with};

const ADDR_SYNC_TIME_0: u16 = 0x10;
const SETTLE: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const FAILSAFE_TIMEOUT: Duration = Duration::from_millis(500);

async fn open_monitoring(num_devices: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let client = open_with(&emulator, &monitoring_option(&emulator), config()).await;
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
        client.heartbeat().unwrap();
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_until_lost(client: &Client, device: usize) {
    wait_until(client, "a lost device", |states| {
        states[device] == DeviceState::Lost
    })
    .await;
}

fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

fn failsafes(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().failsafe()).collect())
}

async fn wait_until_failsafe(emulator: &UdpEmulator, client: &Client, device: usize) {
    let give_up = Instant::now() + PATIENCE;
    while !failsafes(emulator)[device] {
        assert!(Instant::now() < give_up, "the failsafe did not trip");
        client.heartbeat().unwrap();
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn cut_off_until_failsafe(emulator: &UdpEmulator, client: &Client, device: usize) {
    client
        .send(SetCpuConfig {
            config: CpuConfig {
                failsafe_timeout: Some(FAILSAFE_TIMEOUT),
                ..CpuConfig::default()
            },
        })
        .await
        .unwrap();
    emulator.set_muted(device, true);
    wait_until_lost(client, device).await;
    wait_until_failsafe(emulator, client, device).await;
    emulator.set_muted(device, false);
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
async fn a_device_that_answers_again_stays_lost_until_recovered() {
    let (emulator, client) = open_monitoring(2).await;

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    emulator.set_muted(1, false);
    tokio::time::sleep(SETTLE).await;

    assert_eq!(states(&client)[1], DeviceState::Lost);
    assert_eq!(client.bus_stats().recoveries(), 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn recover_brings_back_a_device_that_answers_again_and_it_takes_frames() {
    let (emulator, client) = open_monitoring(3).await;

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;
    client.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true, false, true]);
    emulator.set_muted(1, false);

    assert_eq!(
        client.recover().await.unwrap(),
        DeviceMask::from_bits(0b010)
    );
    assert_ne!(states(&client)[1], DeviceState::Lost);

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(fans(&emulator), [true, true, true]);
    assert_eq!(client.bus_stats().recoveries(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn recover_with_nothing_lost_brings_back_nothing() {
    let (_emulator, client) = open_monitoring(2).await;
    let resets = client.bus_stats().resets();

    assert_eq!(client.recover().await.unwrap(), DeviceMask::default());

    assert_eq!(client.bus_stats().resets(), resets);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn recover_leaves_a_device_that_is_still_silent_lost() {
    let (emulator, client) = open_monitoring(2).await;

    emulator.set_muted(1, true);
    wait_until_lost(&client, 1).await;

    assert_eq!(client.recover().await.unwrap(), DeviceMask::default());

    assert_eq!(states(&client)[1], DeviceState::Lost);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rebooted_device_is_not_recovered() {
    let (emulator, client) = open_monitoring(3).await;

    emulator.reboot(1);
    wait_until_lost(&client, 1).await;

    assert_eq!(client.recover().await.unwrap(), DeviceMask::default());

    assert_eq!(states(&client)[1], DeviceState::Lost);
    assert_eq!(client.bus_stats().recoveries(), 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_device_cut_off_at_once_is_recovered() {
    let (emulator, client) = open_monitoring(3).await;

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

    assert_eq!(
        client.recover().await.unwrap(),
        DeviceMask::from_bits(0b111)
    );

    client.send(ForceFan { value: true }).await.unwrap();

    assert_eq!(fans(&emulator), [true, true, true]);
    assert_eq!(client.bus_stats().recoveries(), 3);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_recovered_device_is_neither_synchronized_nor_released_by_itself() {
    let (emulator, client) = open_monitoring(3).await;
    let before = sync_times(&emulator);

    cut_off_until_failsafe(&emulator, &client, 1).await;
    assert_eq!(
        client.recover().await.unwrap(),
        DeviceMask::from_bits(0b010)
    );
    client.send(Nop).await.unwrap();

    assert_eq!(sync_times(&emulator), before);
    assert_eq!(failsafes(&emulator), [false, true, false]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_user_synchronizes_and_releases_only_the_recovered_device() {
    let (emulator, client) = open_monitoring(3).await;
    let before = sync_times(&emulator);

    cut_off_until_failsafe(&emulator, &client, 1).await;
    let recovered = client.recover().await.unwrap();
    wait_until(&client, "every device ready", |states| {
        states.iter().all(|&state| state == DeviceState::Ready)
    })
    .await;
    client
        .send(each(|device| {
            recovered[device.idx()].then_some(Synchronize)
        }))
        .await
        .unwrap();
    client.send(ReleaseFailsafe).await.unwrap();

    let after = sync_times(&emulator);
    assert_eq!(after[0], before[0]);
    assert_ne!(after[1], before[1]);
    assert_eq!(after[2], before[2]);
    assert_eq!(failsafes(&emulator), [false, false, false]);
    client.close().await.unwrap();
}

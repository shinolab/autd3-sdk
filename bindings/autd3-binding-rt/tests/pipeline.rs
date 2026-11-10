use std::time::Duration;

use autd3_binding_rt::Client;
use autd3_rs::commands::{CpuConfig, Modulation, PtpConfig, SetCpuConfig};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::TransportOption;
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Error, Frames};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

const PATIENCE: Duration = Duration::from_secs(10);

async fn open(emulator: &UdpEmulator) -> Client {
    let geometry = Geometry::new((0..emulator.num_devices()).map(|_| Autd3::default()));
    let option = TransportOption {
        iface: emulator.interface(),
        ..TransportOption::default()
    };
    Client::open(&geometry, &option).await.unwrap().0
}

fn modulation(client: &Client) -> Frames {
    Frames::encode(
        client.geometry(),
        Modulation::new(SamplingConfig::FREQ_4K, &[0x80; 2000]),
    )
    .unwrap()
}

fn rejected(client: &Client) -> Frames {
    Frames::encode(
        client.geometry(),
        SetCpuConfig::new(CpuConfig {
            ptp: PtpConfig {
                sync_interval: Duration::ZERO,
                ..PtpConfig::DEFAULT
            },
            ..CpuConfig::DEFAULT
        }),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "depends on real time"]
async fn concurrent_calls_each_get_the_result_of_their_own_frames() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    let (first, second, third) = (modulation(&client), rejected(&client), modulation(&client));

    let (first, second, third) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            client.send_streaming(&first),
            client.send_streaming(&second),
            client.send(&third),
        )
    })
    .await
    .unwrap();

    first.unwrap();
    assert!(matches!(second, Err(Error::DeviceError { device: 0, .. })));
    third.unwrap();
    let state = tokio::time::timeout(PATIENCE, client.read_state())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.replies().len(), 2);
    tokio::time::timeout(PATIENCE, client.read_firmware_version())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "depends on real time"]
async fn a_cancelled_call_does_not_hold_back_the_calls_after_it() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    let frames = modulation(&client);

    for _ in 0..3 {
        let _ = tokio::time::timeout(Duration::ZERO, client.send_streaming(&frames)).await;
    }

    tokio::time::timeout(PATIENCE, client.send(&frames))
        .await
        .unwrap()
        .unwrap();
}

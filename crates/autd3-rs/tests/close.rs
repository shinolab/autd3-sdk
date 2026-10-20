mod common;

use autd3_rs::commands::{FixedCompletionTime, ForceFan, Nop, Pattern, SetSilencer};
use autd3_rs::udp::UdpError;
use autd3_rs::value::Intensity;
use autd3_rs::{Client, Error, Frames};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{emulator, open, open_with, patient_option};

fn intensities(emulator: &UdpEmulator, device: usize) -> Vec<Intensity> {
    emulator.with_device(device, |d| d.fpga().emissions().1)
}

async fn emit(client: &Client) {
    let phases = client.geometry().phase_buffer();
    client
        .send(Pattern::new(&phases, Intensity::MAX))
        .await
        .unwrap();
}

fn stop_frames(client: &Client) -> u64 {
    let geometry = client.geometry();
    let phases = geometry.phase_buffer();
    let silencer = Frames::encode(
        geometry,
        SetSilencer::new(FixedCompletionTime {
            strict_mode: false,
            ..Default::default()
        }),
    )
    .unwrap();
    let pattern = Frames::encode(geometry, Pattern::new(&phases, Intensity::MIN)).unwrap();
    u64::try_from(silencer.len() + pattern.len()).unwrap()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn close_silences_every_device() {
    let (emulator, client) = open(2).await;
    emit(&client).await;
    for device in 0..2 {
        assert!(
            intensities(&emulator, device)
                .iter()
                .all(|&i| i == Intensity::MAX)
        );
    }
    let frames = client.bus_stats().frames();

    client.close().await.unwrap();

    for device in 0..2 {
        assert!(
            intensities(&emulator, device)
                .iter()
                .all(|&i| i == Intensity::MIN)
        );
    }
    assert_eq!(client.bus_stats().frames() - frames, stop_frames(&client));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn everything_fails_with_closed_after_a_close() {
    let (_emulator, client) = open(1).await;
    let checker = client.state_checker();
    let frames = Frames::encode(client.geometry(), Nop).unwrap();
    client.close().await.unwrap();
    let sent = client.bus_stats().frames();

    assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
    assert!(matches!(
        client.send_frame(frames.frame(0).unwrap()).await.err(),
        Some(Error::Closed)
    ));
    assert!(matches!(
        client.read_firmware_version().await,
        Err(Error::Closed)
    ));
    assert!(matches!(client.read_fpga_state().await, Err(Error::Closed)));
    assert!(matches!(client.read_telemetry().await, Err(Error::Closed)));
    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    assert!(matches!(
        client.state_checker().check(),
        Err(UdpError::Closed)
    ));
    assert_eq!(client.bus_stats().frames(), sent);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_second_close_succeeds_without_sending_the_stop_frames_again() {
    let (emulator, client) = open(1).await;
    client.close().await.unwrap();
    let sent = client.bus_stats().frames();
    emulator.with_device(0, |d| assert!(!d.fpga().force_fan()));

    client.close().await.unwrap();
    client.close().await.unwrap();

    assert_eq!(client.bus_stats().frames(), sent);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn concurrent_closes_send_the_stop_frames_once() {
    let (_emulator, client) = open(2).await;
    let frames = client.bus_stats().frames();

    let (first, second) = tokio::join!(client.close(), client.close());

    first.unwrap();
    second.unwrap();
    assert_eq!(client.bus_stats().frames() - frames, stop_frames(&client));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_still_in_flight_resolves_with_closed_when_the_client_closes() {
    let emulator = emulator(1);
    let client = open_with(&emulator, &patient_option(&emulator)).await;
    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();

    emulator.drop_next_replies(0, 1);
    let unanswered = client.send_frame(frames.frame(0).unwrap()).await.unwrap();
    client.close().await.unwrap();

    assert!(matches!(unanswered.await, Err(Error::Closed)));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_still_in_flight_resolves_with_closed_when_the_client_is_dropped() {
    let emulator = emulator(1);
    let client = open_with(&emulator, &patient_option(&emulator)).await;
    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();

    emulator.drop_next_replies(0, 1);
    let unanswered = client.send_frame(frames.frame(0).unwrap()).await.unwrap();
    drop(client);

    assert!(matches!(unanswered.await, Err(Error::Closed)));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn dropping_the_client_closes_the_connection_without_sending_the_stop_frames() {
    let (emulator, client) = open(1).await;
    emit(&client).await;
    let checker = client.state_checker();
    let stats = client.bus_stats();
    let frames = stats.frames();

    drop(client);

    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    assert_eq!(stats.frames(), frames);
    assert!(
        intensities(&emulator, 0)
            .iter()
            .all(|&i| i == Intensity::MAX)
    );
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_close_whose_stop_frame_fails_leaves_the_client_open_for_another_try() {
    let (emulator, client) = open(1).await;
    emit(&client).await;
    let checker = client.state_checker();

    emulator.drop_next_replies(0, 1);
    let failed = client.close().await;

    assert!(matches!(failed, Err(Error::Timeout { .. })));
    assert!(checker.check().is_ok());
    client.send(Nop).await.unwrap();

    client.close().await.unwrap();

    assert!(matches!(checker.check(), Err(UdpError::Closed)));
    assert!(
        intensities(&emulator, 0)
            .iter()
            .all(|&i| i == Intensity::MIN)
    );
}

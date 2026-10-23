mod common;

use autd3_rs::commands::{ForceFan, Nop, Pattern};
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

#[tokio::test]
#[ignore = "depends on real time"]
async fn close_sends_nothing_and_leaves_the_output_as_it_is() {
    let (emulator, client) = open(2);
    emit(&client).await;
    let frames = emulator.frames_received(0);

    client.close().unwrap();

    assert!(matches!(client.device_status(), Err(Error::Closed)));
    assert_eq!(emulator.frames_received(0), frames);
    for device in 0..2 {
        assert!(
            intensities(&emulator, device)
                .iter()
                .all(|&i| i == Intensity::MAX)
        );
    }
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn everything_fails_with_closed_after_a_close() {
    let (emulator, client) = open(1);
    let frames = Frames::encode(client.geometry(), Nop).unwrap();
    client.close().unwrap();
    let sent = emulator.frames_received(0);

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
    assert!(matches!(client.device_status(), Err(Error::Closed)));
    assert!(matches!(client.send_heartbeat(), Err(Error::Closed)));
    assert_eq!(emulator.frames_received(0), sent);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_still_in_flight_resolves_with_closed_when_the_client_closes() {
    let emulator = emulator(1);
    let client = open_with(&emulator, &patient_option(&emulator));
    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();

    emulator.drop_next_replies(0, 1);
    let unanswered = client.send_frame(frames.frame(0).unwrap()).await.unwrap();
    client.close().unwrap();

    assert!(matches!(unanswered.await, Err(Error::Closed)));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_still_in_flight_resolves_with_closed_when_the_client_is_dropped() {
    let emulator = emulator(1);
    let client = open_with(&emulator, &patient_option(&emulator));
    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();

    emulator.drop_next_replies(0, 1);
    let unanswered = client.send_frame(frames.frame(0).unwrap()).await.unwrap();
    drop(client);

    assert!(matches!(unanswered.await, Err(Error::Closed)));
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn dropping_the_client_closes_the_connection_without_sending_anything() {
    let (emulator, client) = open(1);
    emit(&client).await;
    let frames = emulator.frames_received(0);

    drop(client);

    assert_eq!(emulator.frames_received(0), frames);
    assert!(
        intensities(&emulator, 0)
            .iter()
            .all(|&i| i == Intensity::MAX)
    );
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_second_close_reports_the_closed_client() {
    let (emulator, client) = open(1);
    client.close().unwrap();
    let sent = emulator.frames_received(0);
    emulator.with_device(0, |d| assert!(!d.fpga().force_fan()));

    assert!(matches!(client.close(), Err(Error::Closed)));

    assert_eq!(emulator.frames_received(0), sent);
}

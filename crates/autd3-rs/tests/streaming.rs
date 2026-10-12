mod common;

use std::num::NonZeroUsize;

use autd3_rs::commands::{Modulation, Nop};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig, Error, Frames, MAX_INFLIGHT, PayloadError};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{
    INVALID_TRANSITION_MODE, REJECTED_BY_THE_DEVICE, config, emulator, open, open_with, option,
};

fn samples(len: u32) -> Vec<u8> {
    (0..len).map(|i| i.to_le_bytes()[0] ^ 0x5A).collect()
}

async fn open_windowed(num_devices: usize, max_inflight: usize) -> (UdpEmulator, Client) {
    let emulator = emulator(num_devices);
    let config = ClientConfig {
        max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
        ..config()
    };
    let client = open_with(&emulator, &option(&emulator), config).await;
    (emulator, client)
}

#[tokio::test]
async fn a_streamed_command_arrives_whole_on_every_device() {
    let (emulator, client) = open(2).await;
    let data = samples(30_000);
    let command = Modulation::new(SamplingConfig::FREQ_4K, &data);
    let frames = Frames::encode(client.geometry(), command).unwrap();
    assert!(frames.len() > MAX_INFLIGHT);
    let before = client.bus_stats().frames();

    let stream = client.send_streaming(command).await.unwrap();
    stream.await.unwrap();

    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(frames.len()).unwrap()
    );
    for device in 0..2 {
        let written = emulator.with_device(device, |d| d.fpga().modulation_buffer(0));
        assert_eq!(written, data);
    }
    let telemetry = client.read_telemetry().await.unwrap();
    assert!(telemetry.iter().all(|t| t.fifo_drop == 0));
    assert_eq!(client.bus_stats().resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_stream_never_has_more_frames_in_flight_than_the_window() {
    for window in [1, 3, MAX_INFLIGHT] {
        let (emulator, client) = open_windowed(1, window).await;
        let data = samples(30_000);
        let before = client.bus_stats().frames();

        emulator.drop_next_replies(0, 1);
        let stream = client
            .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &data))
            .await
            .unwrap();

        assert_eq!(
            client.bus_stats().frames() - before,
            u64::try_from(window).unwrap()
        );
        assert!(matches!(stream.await, Err(Error::Timeout { .. })));
        client.send(Nop).await.unwrap();
        assert_eq!(client.bus_stats().resets(), 2);
        client.close().await.unwrap();
    }
}

#[tokio::test]
async fn a_stream_stops_at_the_frame_a_device_rejects() {
    let (_emulator, client) = open_windowed(2, 1).await;
    let before = client.bus_stats().frames();

    let stream = client
        .send_streaming((Nop, REJECTED_BY_THE_DEVICE, Nop, Nop))
        .await
        .unwrap();

    assert_eq!(client.bus_stats().frames() - before, 2);
    assert!(matches!(
        stream.await,
        Err(Error::DeviceError {
            device: 0,
            code: INVALID_TRANSITION_MODE
        })
    ));
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_stream_inside_the_window_reports_the_rejection_after_sending_everything() {
    let (_emulator, client) = open(2).await;
    let before = client.bus_stats().frames();

    let stream = client
        .send_streaming((Nop, REJECTED_BY_THE_DEVICE, Nop, Nop))
        .await
        .unwrap();
    let outcome = stream.await;

    assert_eq!(client.bus_stats().frames() - before, 4);
    assert!(matches!(
        outcome,
        Err(Error::DeviceError {
            device: 0,
            code: INVALID_TRANSITION_MODE
        })
    ));
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_stream_reports_the_failure_of_its_earliest_frame() {
    let (emulator, client) = open(1).await;

    emulator.drop_next_frames(0, 1);
    let stream = client.send_streaming((Nop, Nop, Nop)).await.unwrap();

    assert!(matches!(stream.await, Err(Error::Timeout { .. })));
    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_command_that_cannot_be_encoded_sends_nothing() {
    let (_emulator, client) = open(1).await;
    let before = client.bus_stats().frames();

    let streamed = client
        .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &[]))
        .await;
    let sent = client
        .send(Modulation::new(SamplingConfig::FREQ_4K, &[]))
        .await;

    assert!(matches!(
        streamed.err(),
        Some(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
    ));
    assert!(matches!(
        sent,
        Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
    ));
    assert_eq!(client.bus_stats().frames(), before);
    client.close().await.unwrap();
}

#[tokio::test]
async fn several_streams_can_be_awaited_after_all_of_them_were_sent() {
    let (emulator, client) = open(2).await;
    let first = samples(5_000);
    let second = samples(7_000);

    let a = client
        .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &first))
        .await
        .unwrap();
    let b = client
        .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &second))
        .await
        .unwrap();
    b.await.unwrap();
    a.await.unwrap();

    let written = emulator.with_device(1, |d| d.fpga().modulation_buffer(0));
    assert_eq!(written, second);
    client.close().await.unwrap();
}

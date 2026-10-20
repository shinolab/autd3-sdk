mod common;

use std::pin::pin;
use std::task::{Context, Waker};

use autd3_rs::commands::{Modulation, Nop};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Error, Frames, MAX_INFLIGHT, PayloadError};

use common::{INVALID_TRANSITION_MODE, REJECTED_BY_THE_DEVICE, emulator, open, open_with, option};

fn samples(len: u32) -> Vec<u8> {
    (0..len).map(|i| i.to_le_bytes()[0] ^ 0x5A).collect()
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn frames_sent_without_waiting_arrive_whole_on_every_device() {
    let (emulator, client) = open(2).await;
    let data = samples(30_000);
    let frames = Frames::encode(
        client.geometry(),
        Modulation::new(SamplingConfig::FREQ_4K, &data),
    )
    .unwrap();
    assert!(frames.len() > MAX_INFLIGHT);
    let before = client.bus_stats().frames();

    let mut responses = Vec::with_capacity(frames.len());
    for frame in &frames {
        responses.push(client.send_frame(frame).await.unwrap());
    }
    for response in responses {
        response.await.unwrap().check().unwrap();
    }

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
#[ignore = "depends on real time"]
async fn send_frame_waits_while_the_window_is_full() {
    let emulator = emulator(1);
    let client = open_with(&emulator, &option(&emulator)).await;
    let nop = Frames::encode(client.geometry(), Nop).unwrap();
    let before = client.bus_stats().frames();

    emulator.pause_main_loop(0);
    let mut queued = Vec::with_capacity(MAX_INFLIGHT);
    for _ in 0..MAX_INFLIGHT {
        queued.push(client.send_frame(nop.frame(0).unwrap()).await.unwrap());
    }
    {
        let mut next = pin!(client.send_frame(nop.frame(0).unwrap()));
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }

    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(MAX_INFLIGHT).unwrap()
    );
    for response in queued {
        assert!(matches!(response.await, Err(Error::Timeout { .. })));
    }
    emulator.resume_main_loop(0);
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), 2);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rejected_frame_sent_without_waiting_fails_only_its_own_response() {
    let (_emulator, client) = open(2).await;
    let frames =
        Frames::encode(client.geometry(), (Nop, REJECTED_BY_THE_DEVICE, Nop, Nop)).unwrap();

    let mut responses = Vec::with_capacity(frames.len());
    for frame in &frames {
        responses.push(client.send_frame(frame).await.unwrap());
    }
    let mut outcomes = Vec::with_capacity(responses.len());
    for response in responses {
        outcomes.push(response.await.unwrap().check());
    }

    assert!(outcomes[0].is_ok());
    assert!(matches!(
        outcomes[1],
        Err(Error::DeviceError {
            device: 0,
            code: INVALID_TRANSITION_MODE
        })
    ));
    assert!(outcomes[2..].iter().all(Result::is_ok));
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_command_that_cannot_be_encoded_sends_nothing() {
    let (_emulator, client) = open(1).await;
    let before = client.bus_stats().frames();

    let sent = client
        .send(Modulation::new(SamplingConfig::FREQ_4K, &[]))
        .await;

    assert!(matches!(
        sent,
        Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
    ));
    assert_eq!(client.bus_stats().frames(), before);
    client.close().await.unwrap();
}

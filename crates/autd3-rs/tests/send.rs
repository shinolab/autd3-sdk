mod common;

use std::time::Duration;

use autd3_rs::commands::{Distribution, ForceFan, Modulation, Nop, each};
use autd3_rs::protocol::{Cmd, PAYLOAD_BYTES};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{ClientConfig, Datagram, Error, ExecAt, Frame, Frames, PayloadError, Version};
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use common::{
    INVALID_TRANSITION_MODE, REJECTED_BY_THE_DEVICE, config, emulator, geometry, open, open_with,
};

const FRAMES_SENT_BY_OPEN: u64 = 3;

fn fans(emulator: &UdpEmulator) -> Vec<bool> {
    emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().force_fan()).collect())
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_single_device_receives_what_is_sent() {
    let (emulator, client) = open(1).await;
    assert_eq!(client.num_devices(), 1);
    assert_eq!(client.geometry().num_devices(), 1);
    assert_eq!(fans(&emulator), [false]);

    client.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true]);

    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_emulator_processes_frames_with_exec_lead_without_waiting() {
    let emulator = emulator(3);
    let config = ClientConfig {
        exec_lead: Some(Duration::from_millis(5)),
        ..config()
    };
    let client = open_with(&emulator, &common::option(&emulator), config).await;

    client.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true, true, true]);
    let telemetry = client.read_telemetry().await.unwrap();
    assert!(telemetry.iter().all(|t| t.exec_late == 0), "{telemetry:?}");

    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_sent_with_an_overridden_execution_time_completes() {
    let emulator = emulator(2);
    let config = ClientConfig {
        exec_lead: Some(Duration::from_millis(5)),
        ..config()
    };
    let client = open_with(&emulator, &common::option(&emulator), config).await;
    let on = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    let off = Frames::encode(client.geometry(), ForceFan { value: false }).unwrap();

    client
        .send_frame_at(on.frame(0).unwrap(), ExecAt::Immediate)
        .await
        .unwrap()
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(fans(&emulator), [true, true]);
    client
        .send_frame_at(
            off.frame(0).unwrap(),
            ExecAt::Lead(Duration::from_millis(1)),
        )
        .await
        .unwrap()
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(fans(&emulator), [false, false]);
    let at = client.device_time_now().unwrap() + Duration::from_millis(1);
    client
        .send_frame_at(on.frame(0).unwrap(), ExecAt::At(at))
        .await
        .unwrap()
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(fans(&emulator), [true, true]);

    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_rejected_execution_time_sends_nothing() {
    let (_emulator, client) = open(1).await;
    let frames = Frames::encode(client.geometry(), Nop).unwrap();
    let before = client.bus_stats().frames();

    let error = client
        .send_frame_at(
            frames.frame(0).unwrap(),
            ExecAt::Lead(Duration::from_micros(1)),
        )
        .await
        .err()
        .unwrap();

    assert!(matches!(
        error,
        Error::InvalidPayload(PayloadError::ExecLeadTooShort { .. })
    ));
    assert_eq!(client.bus_stats().frames(), before);
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_broadcast_frame_reaches_every_device() {
    let (emulator, client) = open(3).await;

    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.distribution(), Distribution::Broadcast);
    let response = client.send_frame(frame).await.unwrap().await.unwrap();

    assert_eq!(response.status(), [0, 0, 0]);
    assert_eq!(fans(&emulator), [true, true, true]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_per_device_frame_delivers_each_device_its_own_payload() {
    let (emulator, client) = open(3).await;

    let command = each(|device| {
        Some(ForceFan {
            value: device.idx() == 1,
        })
    });
    let frames = Frames::encode(client.geometry(), command).unwrap();
    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.distribution(), Distribution::PerDevice);
    let response = client.send_frame(frame).await.unwrap().await.unwrap();

    assert_eq!(response.status(), [0, 0, 0]);
    assert_eq!(fans(&emulator), [false, true, false]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_device_left_out_of_a_per_device_command_keeps_its_state() {
    let (emulator, client) = open(2).await;
    client.send(ForceFan { value: true }).await.unwrap();

    client
        .send(each(|device| {
            (device.idx() == 0).then_some(ForceFan { value: false })
        }))
        .await
        .unwrap();

    assert_eq!(fans(&emulator), [false, true]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_command_spanning_many_frames_arrives_whole_on_every_device() {
    let (emulator, client) = open(2).await;
    let data: Vec<u8> = (0..20_000u32).map(|i| i.to_le_bytes()[0]).collect();
    let command = Modulation::new(SamplingConfig::FREQ_4K, &data);
    let frames = Frames::encode(client.geometry(), command).unwrap();
    assert!(frames.len() > 3);
    let before = client.bus_stats().frames();

    client.send(command).await.unwrap();

    assert_eq!(
        client.bus_stats().frames() - before,
        u64::try_from(frames.len()).unwrap()
    );
    for device in 0..2 {
        let written = emulator.with_device(device, |d| d.fpga().modulation_buffer(0));
        assert_eq!(written, data);
    }
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_statistics_count_the_frames_and_their_acknowledgements() {
    let (_emulator, client) = open(2).await;
    let stats = client.bus_stats();
    assert_eq!(stats.resets(), 1);
    assert_eq!(stats.frames(), FRAMES_SENT_BY_OPEN);
    assert_eq!(stats.heartbeats(), 0);

    for _ in 0..5 {
        client.send(Nop).await.unwrap();
    }

    assert_eq!(stats.frames(), FRAMES_SENT_BY_OPEN + 5);
    assert_eq!(stats.acked_frames(), stats.resets() + stats.frames());
    assert_eq!(stats.resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_firmware_version_is_read_from_every_device() {
    let (_emulator, client) = open(2).await;

    let versions = client.read_firmware_version().await.unwrap();

    assert_eq!(versions.len(), 2);
    for version in versions {
        assert_eq!(
            version.cpu,
            Version {
                major: FW_VERSION_MAJOR,
                minor: FW_VERSION_MINOR,
                patch: FW_VERSION_PATCH,
            }
        );
        assert!(!version.fpga.is_unknown());
        assert!(version.is_emulator());
        assert!(version.is_supported());
    }
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_fpga_state_is_read_from_every_device() {
    let (emulator, client) = open(2).await;

    let states = client.read_fpga_state().await.unwrap();

    let expected: Vec<u8> =
        emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().fpga_state()).collect());
    assert_eq!(states.iter().map(|s| s.0).collect::<Vec<_>>(), expected);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_telemetry_counts_the_frames_each_device_processed() {
    let (_emulator, client) = open(2).await;
    let before = client.read_telemetry().await.unwrap();
    assert_eq!(before.len(), 2);

    for _ in 0..4 {
        client.send(Nop).await.unwrap();
    }

    let after = client.read_telemetry().await.unwrap();
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(after.processed - before.processed, 5);
        assert_eq!(after.dedup, 0);
        assert_eq!(after.seq_mismatch, 0);
        assert_eq!(after.fifo_drop, 0);
    }
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_error_reported_by_a_device_fails_the_send_without_desynchronizing() {
    let (_emulator, client) = open(2).await;

    let error = client.send(REJECTED_BY_THE_DEVICE).await.unwrap_err();

    assert!(matches!(
        error,
        Error::DeviceError {
            device: 0,
            code: INVALID_TRANSITION_MODE
        }
    ));
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), 1);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_per_device_error_names_the_device_that_reported_it() {
    let (_emulator, client) = open(3).await;

    let error = client
        .send(each(|device| {
            (device.idx() == 2).then_some(REJECTED_BY_THE_DEVICE)
        }))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::DeviceError {
            device: 2,
            code: INVALID_TRANSITION_MODE
        }
    ));
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_response_of_a_frame_carries_the_status_of_every_device() {
    let (_emulator, client) = open(3).await;
    let frames = Frames::encode(
        client.geometry(),
        each(|device| (device.idx() == 1).then_some(REJECTED_BY_THE_DEVICE)),
    )
    .unwrap();

    let response = client
        .send_frame(frames.frame(0).unwrap())
        .await
        .unwrap()
        .await
        .unwrap();

    assert_eq!(response.status(), [0, INVALID_TRANSITION_MODE, 0]);
    assert!(matches!(
        response.check(),
        Err(Error::DeviceError { device: 1, .. })
    ));
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_encoded_for_another_geometry_is_rejected_before_it_is_sent() {
    let (_emulator, client) = open(1).await;
    let frames = Frames::encode(&geometry(2), each(|_| Some(Nop))).unwrap();
    let before = client.bus_stats().frames();

    let error = client
        .send_frame(frames.frame(0).unwrap())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        error,
        Error::InvalidPayload(PayloadError::DatagramCountMismatch {
            expected: 1,
            got: 2
        })
    ));
    assert_eq!(client.bus_stats().frames(), before);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_rebuilt_from_its_datagrams_is_sent_like_the_original() {
    let (emulator, client) = open(2).await;
    let frames = Frames::encode(
        client.geometry(),
        each(|device| (device.idx() == 1).then_some(ForceFan { value: true })),
    )
    .unwrap();
    let encoded = frames.frame(0).unwrap();
    let datagrams = encoded.datagrams().to_vec();

    client
        .send_frame(Frame::new(encoded.distribution(), &datagrams))
        .await
        .unwrap()
        .await
        .unwrap()
        .check()
        .unwrap();

    assert_eq!(fans(&emulator), [false, true]);
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_built_with_malformed_datagrams_is_rejected_before_it_is_sent() {
    let (_emulator, client) = open(2).await;
    let nop = Datagram::no_payload(Cmd::Nop);
    let before = client.bus_stats().frames();

    let broadcast = client
        .send_frame(Frame::new(
            Distribution::Broadcast,
            &[nop.clone(), nop.clone()],
        ))
        .await
        .err()
        .unwrap();
    let per_device = client
        .send_frame(Frame::new(
            Distribution::PerDevice,
            std::slice::from_ref(&nop),
        ))
        .await
        .err()
        .unwrap();
    let oversized = Datagram {
        payload_len: PAYLOAD_BYTES + 1,
        ..nop.clone()
    };
    let too_long = client
        .send_frame(Frame::new(Distribution::PerDevice, &[nop, oversized]))
        .await
        .err()
        .unwrap();

    assert!(matches!(
        broadcast,
        Error::InvalidPayload(PayloadError::DatagramCountMismatch {
            expected: 1,
            got: 2
        })
    ));
    assert!(matches!(
        per_device,
        Error::InvalidPayload(PayloadError::DatagramCountMismatch {
            expected: 2,
            got: 1
        })
    ));
    assert!(matches!(
        too_long,
        Error::InvalidPayload(PayloadError::DatagramPayloadTooLong {
            index: 1,
            len,
            max: PAYLOAD_BYTES
        }) if len == PAYLOAD_BYTES + 1
    ));
    assert_eq!(client.bus_stats().frames(), before);
    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn concurrent_senders_all_complete() {
    let (_emulator, client) = open(2).await;
    let before = client.bus_stats().frames();

    let (nop, versions, fan, telemetry, another_nop) = tokio::join!(
        client.send(Nop),
        client.read_firmware_version(),
        client.send(ForceFan { value: true }),
        client.read_telemetry(),
        client.send(Nop),
    );
    nop.unwrap();
    assert_eq!(versions.unwrap().len(), 2);
    fan.unwrap();
    assert_eq!(telemetry.unwrap().len(), 2);
    another_nop.unwrap();

    assert_eq!(client.bus_stats().frames() - before, 5);
    client.close().await.unwrap();
}

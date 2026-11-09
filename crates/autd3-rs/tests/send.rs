mod common;

use std::time::Duration;

use autd3_rs::commands::{Distribution, ForceFan, Nop, each};
use autd3_rs::{Error, ExecAt, Frame, Frames, Version};
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};

use common::{INVALID_PAYLOAD, REJECTED_BY_THE_DEVICE, fans, open, send_and_collect};

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_single_device_receives_what_is_sent() {
    let (emulator, client) = open(1).await;
    assert_eq!(client.num_devices(), 1);
    assert_eq!(client.geometry().num_devices(), 1);
    assert_eq!(fans(&emulator), [false]);

    client.send(ForceFan { value: true }).await.unwrap();
    assert_eq!(fans(&emulator), [true]);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn the_emulator_processes_frames_with_an_execution_time_without_waiting() {
    let (emulator, client) = open(3).await;
    let on = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();

    let response = send_and_collect(
        &client,
        on.frame(0).unwrap(),
        ExecAt::At(client.device_time_now().unwrap() + Duration::from_secs(5)),
    )
    .await;
    response.check().unwrap();
    assert_eq!(fans(&emulator), [true, true, true]);
    assert!(
        response
            .replies()
            .iter()
            .all(|r| r.telemetry.exec_late == 0),
        "{response:?}"
    );
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_frame_sent_with_each_execution_time_completes() {
    let (emulator, client) = open(2).await;
    let on = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    let off = Frames::encode(client.geometry(), ForceFan { value: false }).unwrap();

    send_and_collect(&client, on.frame(0).unwrap(), ExecAt::Immediate)
        .await
        .check()
        .unwrap();
    assert_eq!(fans(&emulator), [true, true]);
    let at = client.device_time_now().unwrap() + Duration::from_millis(1);
    send_and_collect(&client, off.frame(0).unwrap(), ExecAt::At(at))
        .await
        .check()
        .unwrap();
    assert_eq!(fans(&emulator), [false, false]);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn a_broadcast_frame_reaches_every_device() {
    let (emulator, client) = open(3).await;

    let frames = Frames::encode(client.geometry(), ForceFan { value: true }).unwrap();
    assert_eq!(frames.len(), 1);
    let frame = frames.frame(0).unwrap();
    assert_eq!(frame.distribution(), Distribution::Broadcast);
    let response = send_and_collect(&client, frame, ExecAt::Immediate).await;

    assert_eq!(
        response
            .replies()
            .iter()
            .map(|r| r.status)
            .collect::<Vec<_>>(),
        [0, 0, 0]
    );
    assert_eq!(fans(&emulator), [true, true, true]);
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
    let response = send_and_collect(&client, frame, ExecAt::Immediate).await;

    assert_eq!(
        response
            .replies()
            .iter()
            .map(|r| r.status)
            .collect::<Vec<_>>(),
        [0, 0, 0]
    );
    assert_eq!(fans(&emulator), [false, true, false]);
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
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_heartbeat_reply_carries_the_fpga_state_of_its_device() {
    let (emulator, client) = open(2).await;

    let response = client
        .collect(client.send_heartbeat().await.unwrap())
        .await
        .unwrap();
    let states: Vec<u8> = response.replies().iter().map(|r| r.fpga_state.0).collect();

    let expected: Vec<u8> =
        emulator.with_devices(|devices| devices.iter().map(|d| d.fpga().fpga_state()).collect());
    assert_eq!(states, expected);
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn every_heartbeat_reply_carries_the_telemetry_of_its_device() {
    let (_emulator, client) = open(2).await;
    for _ in 0..4 {
        client.send(Nop).await.unwrap();
    }

    let response = client
        .collect(client.send_heartbeat().await.unwrap())
        .await
        .unwrap();
    assert_eq!(response.replies().len(), 2);
    assert!(
        response
            .replies()
            .iter()
            .all(|r| r.telemetry.fifo_drop == 0)
    );
}

#[tokio::test]
#[ignore = "depends on real time"]
async fn an_error_reported_by_a_device_fails_only_that_send() {
    let (_emulator, client) = open(2).await;

    let error = client.send(REJECTED_BY_THE_DEVICE).await.unwrap_err();

    assert!(matches!(
        error,
        Error::DeviceError {
            device: 0,
            code: INVALID_PAYLOAD
        }
    ));
    client.send(Nop).await.unwrap();
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
            code: INVALID_PAYLOAD
        }
    ));
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

    let response = send_and_collect(&client, frames.frame(0).unwrap(), ExecAt::Immediate).await;

    assert_eq!(
        response
            .replies()
            .iter()
            .map(|r| r.status)
            .collect::<Vec<_>>(),
        [0, INVALID_PAYLOAD, 0]
    );
    assert!(matches!(
        response.check(),
        Err(Error::DeviceError { device: 1, .. })
    ));
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

    send_and_collect(
        &client,
        Frame::new(encoded.distribution(), &datagrams),
        ExecAt::Immediate,
    )
    .await
    .check()
    .unwrap();

    assert_eq!(fans(&emulator), [false, true]);
}

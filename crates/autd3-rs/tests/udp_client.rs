use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use autd3_rs::commands::{
    Clear, CpuConfig, Modulation, Nop, Pattern, PtpConfig, SetCpuConfig, SetSilencer,
};
use autd3_rs::udp::TransportOption;
use autd3_rs::value::{Intensity, Phase, SamplingConfig, SysTime};
use autd3_rs::{Client, ClientConfig, DeviceState, Error, Frames};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

mod common;
use common::{full_modulation, geometry, open, open_with, option};

#[tokio::test(flavor = "multi_thread")]
async fn a_client_drives_the_emulated_chain() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let geometry = geometry(2);
    let client = open(&emulator).await;
    let checker = client.state_checker();
    assert_eq!(client.num_devices(), 2);
    assert!(client.device_time_now().unwrap() < SysTime::ZERO + Duration::from_secs(60));

    let mut phases = geometry.phase_buffer();
    for (d, device_phases) in phases.iter_mut().enumerate() {
        for (i, p) in device_phases.iter_mut().enumerate() {
            *p = Phase(u8::try_from((i + d * 7) % 256).unwrap());
        }
    }
    let modulation: Vec<u8> = (0..16).map(|i| i * 16).collect();

    client
        .send((
            SetSilencer::default(),
            Pattern::new(&phases, Intensity::MAX),
            Modulation::new(SamplingConfig::FREQ_4K, &modulation),
        ))
        .await
        .unwrap();

    for (d, expected) in phases.iter().enumerate() {
        let (emitted, intensities, buffer) = emulator.with_device(d, |device| {
            let fpga = device.fpga();
            let (p, i) = fpga.emissions();
            (p, i, fpga.modulation_buffer(fpga.current_mod_bank()))
        });
        assert_eq!(&emitted, expected);
        assert!(intensities.iter().all(|&i| i == Intensity::MAX));
        assert_eq!(buffer, modulation);
    }

    let status = checker.check().unwrap();
    assert_eq!(status.devices(), [DeviceState::Ready; 2]);

    client.close().await.unwrap();
    assert!(checker.check().is_err());
    assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ptp_config_that_never_locks_does_not_outlive_the_session() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let never_locks = PtpConfig {
        lock_threshold: Duration::ZERO,
        ..PtpConfig::default()
    };
    let client = open(&emulator).await;
    client
        .send(SetCpuConfig::new(CpuConfig {
            ptp: never_locks,
            ..CpuConfig::default()
        }))
        .await
        .unwrap();
    for unit in 0..3 {
        assert_eq!(emulator.ptp_config(unit), never_locks);
    }
    client.close().await.unwrap();

    let client = open(&emulator).await;
    assert_eq!(
        client.state_checker().check().unwrap().devices(),
        [DeviceState::Ready; 3]
    );
    for unit in 0..3 {
        assert_eq!(emulator.ptp_config(unit), PtpConfig::default());
    }
    client.close().await.unwrap();
}

async fn states_settle_to(client: &Client, expected: &[DeviceState]) -> bool {
    let checker = client.state_checker();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        client.send(Nop).await.unwrap();
        if checker.check().unwrap().devices() == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unit_held_unlocked_by_its_ptp_config_locks_after_a_clear() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    emulator.set_ptp_lock_blocked(1, true);
    client
        .send(SetCpuConfig::new(CpuConfig {
            ptp: PtpConfig {
                lock_threshold: Duration::ZERO,
                ..PtpConfig::default()
            },
            ..CpuConfig::default()
        }))
        .await
        .unwrap();
    emulator.set_ptp_lock_blocked(1, false);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(states_settle_to(&client, &[DeviceState::Ready, DeviceState::Syncing]).await);

    client.send(Clear).await.unwrap();
    assert!(states_settle_to(&client, &[DeviceState::Ready; 2]).await);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_stop_enables_the_silencer_before_muting() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let geometry = geometry(2);
    let client = open(&emulator).await;

    let phases = geometry.phase_buffer();
    client
        .send((
            SetSilencer::disable(),
            Pattern::new(&phases, Intensity::MAX),
        ))
        .await
        .unwrap();
    for d in 0..2 {
        let steps = emulator.with_device(d, |device| {
            device.fpga().silencer_completion_steps_intensity()
        });
        assert_eq!(steps, 1);
    }

    client.silent_stop().await.unwrap();

    for d in 0..2 {
        let (fixed_update_rate, intensity_steps, phase_steps, intensities) =
            emulator.with_device(d, |device| {
                let fpga = device.fpga();
                (
                    fpga.silencer_fixed_update_rate_mode(),
                    fpga.silencer_completion_steps_intensity(),
                    fpga.silencer_completion_steps_phase(),
                    fpga.emissions().1,
                )
            });
        assert!(!fixed_update_rate);
        assert_eq!(intensity_steps, 10);
        assert_eq!(phase_steps, 40);
        assert!(intensities.iter().all(|&i| i == Intensity::MIN));
    }

    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ack_timeout_past_the_clock_range_never_expires() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::MAX,
            ..ClientConfig::default()
        },
    )
    .await;
    client.send((Nop, Nop)).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn transport_timeouts_past_the_clock_range_never_expire() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let option = autd3_rs::udp::TransportOption {
        heartbeat: Some(Duration::MAX),
        reply_timeout: Duration::MAX,
        lost_timeout: Duration::MAX,
        enumeration_timeout: Duration::MAX,
        sync_timeout: Duration::MAX,
        ..common::option(&emulator)
    };
    let client = autd3_rs::Client::open(&geometry(2), &option, ClientConfig::default())
        .await
        .unwrap();
    client.send((Nop, Nop)).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_zero_ack_timeout_is_rejected() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let opened = autd3_rs::Client::open(
        &geometry(1),
        &common::option(&emulator),
        ClientConfig {
            ack_timeout: Duration::ZERO,
            ..ClientConfig::default()
        },
    )
    .await;
    assert!(matches!(
        opened,
        Err(Error::InvalidPayload(
            autd3_rs::error::PayloadError::ZeroAckTimeout
        ))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_modulation_buffer_streams_through_the_emulated_chain() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;

    let modulation = full_modulation();
    let frames = Frames::encode(
        client.geometry(),
        Modulation::new(SamplingConfig::FREQ_4K, &modulation),
    )
    .unwrap();
    assert!(frames.len() > autd3_rs::udp::DEVICE_QUEUE_FRAMES);
    let mut pending = VecDeque::new();
    for frame in &frames {
        pending.push_back(client.send_frame(frame).await.unwrap());
    }
    for future in pending {
        future.await.unwrap().check().unwrap();
    }

    for d in 0..2 {
        let buffer = emulator.with_device(d, |device| {
            let fpga = device.fpga();
            fpga.modulation_buffer(fpga.current_mod_bank())
        });
        assert_eq!(buffer, modulation);
    }
    let stats = client.bus_stats();
    assert_eq!(stats.resets(), 1);
    assert!(stats.acked_frames() >= frames.len() as u64);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reply_that_arrives_while_the_frame_is_still_being_sent_is_counted() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let option = TransportOption {
        send_rate_limit: Some(3.2),
        ..common::option(&emulator)
    };
    let client = Client::open(&geometry(3), &option, ClientConfig::default())
        .await
        .unwrap();

    let buffer = full_modulation();
    client
        .send_streaming(Modulation::new(SamplingConfig::FREQ_4K, &buffer))
        .await
        .unwrap()
        .await
        .unwrap();

    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_window_never_holds_more_frames_than_max_inflight() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let option = autd3_rs::udp::TransportOption {
        lost_timeout: Duration::from_secs(10),
        ..common::option(&emulator)
    };
    let client = autd3_rs::Client::open(
        &geometry(1),
        &option,
        ClientConfig {
            max_inflight: NonZeroUsize::new(3).unwrap(),
            ack_timeout: Duration::from_millis(300),
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    emulator.set_muted(0, true);
    let frames = Frames::encode(client.geometry(), (Nop, Nop, Nop, Nop)).unwrap();
    let mut futures = Vec::new();
    for frame in frames.iter().take(3) {
        futures.push(
            tokio::time::timeout(Duration::from_millis(100), client.send_frame(frame))
                .await
                .expect("the window has room")
                .unwrap(),
        );
    }
    let fourth = frames.frame(3).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), client.send_frame(fourth))
            .await
            .is_err(),
        "the fourth frame must wait for a slot"
    );
    for future in futures {
        assert!(matches!(future.await, Err(Error::Timeout { .. })));
    }
    emulator.set_muted(0, false);
    client.send(Nop).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frame_behind_a_lost_one_fails_with_seq_mismatch_and_the_reset_clears_the_rest() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::from_millis(500),
            ..ClientConfig::default()
        },
    )
    .await;
    let resets_before = client.bus_stats().resets();

    emulator.drop_next_frames(1, 1);
    let frames = Frames::encode(client.geometry(), (Nop, Nop)).unwrap();
    let lost = client.send_frame(frames.frame(0).unwrap()).await.unwrap();
    let behind = client.send_frame(frames.frame(1).unwrap()).await.unwrap();
    match behind.await {
        Err(Error::SeqMismatch {
            device: 1,
            expected,
            got,
        }) => assert_eq!(expected, got.wrapping_add(2)),
        other => panic!("expected a SeqMismatch on device 1, got {other:?}"),
    }

    let started = Instant::now();
    client.send(Nop).await.unwrap();
    assert!(started.elapsed() < Duration::from_millis(400));
    assert_eq!(client.bus_stats().resets(), resets_before + 1);
    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(50), lost).await,
        Ok(Err(Error::Timeout { .. }))
    ));
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), resets_before + 1);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_frame_fails_the_stream_and_the_next_send_recovers_through_a_reset() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::from_millis(100),
            ..ClientConfig::default()
        },
    )
    .await;
    let resets_before = client.bus_stats().resets();

    emulator.drop_next_frames(1, 1);
    let frames = Frames::encode(client.geometry(), (Nop, Nop, Nop)).unwrap();
    let mut futures = Vec::new();
    for frame in &frames {
        futures.push(client.send_frame(frame).await.unwrap());
    }
    let mut results = Vec::new();
    for future in futures {
        results.push(future.await);
    }
    assert!(matches!(results[0], Err(Error::Timeout { .. })));
    assert!(results[1..].iter().all(|r| matches!(
        r,
        Err(Error::SeqMismatch { device: 1, .. } | Error::Timeout { .. })
    )));

    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), resets_before + 1);
    client.send(SetSilencer::default()).await.unwrap();
    assert_eq!(client.bus_stats().resets(), resets_before + 1);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn heartbeats_flow_while_nothing_is_sent() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let client = open(&emulator).await;
    let before = emulator.heartbeats_received(0);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after = emulator.heartbeats_received(0);
    assert!(after - before >= 10, "{before} -> {after}");
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_device_becomes_lost_and_is_skipped_afterwards() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::from_millis(50),
            ..ClientConfig::default()
        },
    )
    .await;
    let checker = client.state_checker();

    emulator.set_muted(1, true);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    while checker.check().unwrap().devices()[1] != DeviceState::Lost {
        assert!(Instant::now() < deadline, "device 1 never became lost");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(checker.check().unwrap().devices()[0], DeviceState::Ready);

    let heartbeats_at_lost = emulator.heartbeats_received(1);
    client.send(Nop).await.unwrap();
    client.send(Nop).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(emulator.heartbeats_received(1), heartbeats_at_lost);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_report_a_lost_device_as_lost_while_writes_keep_going() {
    let emulator = UdpEmulator::spawn(3).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::from_millis(50),
            ..ClientConfig::default()
        },
    )
    .await;
    let checker = client.state_checker();

    assert_eq!(client.read_firmware_version().await.unwrap().len(), 3);
    assert_eq!(client.read_fpga_state().await.unwrap().len(), 3);
    assert_eq!(client.read_telemetry().await.unwrap().len(), 3);

    emulator.set_muted(1, true);
    let deadline = Instant::now() + Duration::from_secs(3);
    while checker.check().unwrap().devices()[1] != DeviceState::Lost {
        assert!(Instant::now() < deadline, "device 1 never became lost");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let versions = client.read_firmware_version().await;
    assert!(
        matches!(versions, Err(Error::DeviceLost { device: 1 })),
        "{versions:?}"
    );
    let state = client.read_fpga_state().await;
    assert!(
        matches!(state, Err(Error::DeviceLost { device: 1 })),
        "{state:?}"
    );
    let telemetry = client.read_telemetry().await;
    assert!(
        matches!(telemetry, Err(Error::DeviceLost { device: 1 })),
        "{telemetry:?}"
    );

    client.send(Nop).await.unwrap();
    client.send_streaming(Nop).await.unwrap().await.unwrap();
    let frames = Frames::encode(client.geometry(), Nop).unwrap();
    let response = client
        .send_frame(frames.frame(0).unwrap())
        .await
        .unwrap()
        .await
        .unwrap();
    response.check().unwrap();
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_mixed_into_the_window_return_their_own_values() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    let frames = Frames::encode(client.geometry(), (Nop, Nop, Nop, Nop)).unwrap();
    let mut futures = Vec::new();
    for frame in &frames {
        futures.push(client.send_frame(frame).await.unwrap());
    }
    let versions = client.read_firmware_version();
    let state = client.read_fpga_state();
    let (versions, state) = tokio::join!(versions, state);
    for future in futures {
        future.await.unwrap().check().unwrap();
    }
    let versions = versions.unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions.iter().all(autd3_rs::FirmwareVersion::is_supported));
    assert_eq!(state.unwrap().len(), 2);
    let telemetry = client.read_telemetry().await.unwrap();
    assert_eq!(telemetry.len(), 2);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rebooted_device_fails_pending_frames_with_a_timeout() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout: Duration::from_millis(50),
            ..ClientConfig::default()
        },
    )
    .await;
    emulator.reboot(1);
    let result = client.send(Nop).await;
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
    let _ = client.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_heartbeats_a_lost_frame_and_a_lost_reset_each_cost_one_ack_timeout() {
    const ACK_TIMEOUT: Duration = Duration::from_millis(50);
    const IDLE_WAIT_MARGIN: Duration = Duration::from_millis(500);
    let emulator = UdpEmulator::spawn(1).unwrap();
    let client = Client::open(
        &geometry(1),
        &TransportOption {
            heartbeat: None,
            lost_timeout: Duration::from_secs(10),
            ..option(&emulator)
        },
        ClientConfig {
            ack_timeout: ACK_TIMEOUT,
            ..ClientConfig::default()
        },
    )
    .await
    .unwrap();
    let resets_before = client.bus_stats().resets();

    emulator.drop_next_frames(0, 2);
    let started = Instant::now();
    let result = client.send(Nop).await;
    let elapsed = started.elapsed();
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
    assert!(elapsed >= ACK_TIMEOUT, "{elapsed:?}");
    assert!(elapsed < IDLE_WAIT_MARGIN, "{elapsed:?}");

    let started = Instant::now();
    client.send(Nop).await.unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed >= ACK_TIMEOUT, "{elapsed:?}");
    assert!(elapsed < IDLE_WAIT_MARGIN, "{elapsed:?}");
    assert_eq!(client.bus_stats().resets(), resets_before + 2);
    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_datagram_duplicated_while_its_frame_is_queued_does_not_fail_the_frame() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    let resets_before = client.bus_stats().resets();

    emulator.duplicate_next_frames(1, 4);
    let frames = Frames::encode(client.geometry(), (Nop, Nop, Nop)).unwrap();
    let mut futures = Vec::new();
    for frame in &frames {
        futures.push(client.send_frame(frame).await.unwrap());
    }
    for future in futures {
        future.await.unwrap();
    }
    client.send(SetSilencer::default()).await.unwrap();
    client.send(Nop).await.unwrap();
    assert_eq!(client.bus_stats().resets(), resets_before);
    client.close().await.unwrap();
}

async fn drive_at_full_intensity_without_the_silencer(client: &Client) {
    let phases = client.geometry().phase_buffer();
    client
        .send((
            SetSilencer::disable(),
            Pattern::new(&phases, Intensity::MAX),
        ))
        .await
        .unwrap();
}

fn is_silently_stopped(emulator: &UdpEmulator) -> bool {
    (0..emulator.num_devices()).all(|d| {
        emulator.with_device(d, |device| {
            let fpga = device.fpga();
            fpga.silencer_completion_steps_intensity() == 10
                && fpga.silencer_completion_steps_phase() == 40
                && fpga.emissions().1.iter().all(|&i| i == Intensity::MIN)
        })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_after_a_cancelled_close_still_stops_the_output() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let ack_timeout = Duration::from_millis(100);
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout,
            ..ClientConfig::default()
        },
    )
    .await;
    drive_at_full_intensity_without_the_silencer(&client).await;

    emulator.drop_next_frames(1, 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), client.close())
            .await
            .is_err()
    );
    assert!(!is_silently_stopped(&emulator));
    tokio::time::sleep(ack_timeout * 3).await;

    client.close().await.unwrap();
    assert!(is_silently_stopped(&emulator));
    assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_closes_both_wait_for_the_stop_frames() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let client = open(&emulator).await;
    drive_at_full_intensity_without_the_silencer(&client).await;

    let (first, second) = tokio::join!(client.close(), client.close());
    first.unwrap();
    second.unwrap();
    assert!(is_silently_stopped(&emulator));
    assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_whose_stop_failed_reports_it_and_the_next_close_stops_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let ack_timeout = Duration::from_millis(100);
    let client = open_with(
        &emulator,
        ClientConfig {
            ack_timeout,
            ..ClientConfig::default()
        },
    )
    .await;
    drive_at_full_intensity_without_the_silencer(&client).await;

    emulator.drop_next_frames(1, 1);
    assert!(client.close().await.is_err());
    tokio::time::sleep(ack_timeout * 3).await;

    client.close().await.unwrap();
    assert!(is_silently_stopped(&emulator));
    client.close().await.unwrap();
    assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
}

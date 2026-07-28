#![cfg(feature = "legacy")]

use core::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use autd3_rs::commands::{
    ChangeModulationBank, Clear, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate, FociStm,
    FociStmOption, ForceFan, GpioOut, Modulation, Pattern, PatternStm, PatternStmMode,
    PatternStmOption, SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable,
    SetSilencer, Synchronize,
};
use autd3_rs::legacy::emulator::{
    LegacyAudit, LegacyDevice, LegacyDeviceHandle, StmKind, default_pulse_width_table,
};
use autd3_rs::legacy::error::{
    INVALID_GAIN_STM_MODE, INVALID_SEGMENT_TRANSITION, INVALID_SILENCER_SETTINGS,
    INVALID_TRANSITION_MODE, MISS_TRANSITION_TIME, TimeoutPhase,
};
use autd3_rs::legacy::op;
use autd3_rs::legacy::{
    LegacyChangePatternBank, LegacyClient, LegacyClientConfig, LegacyCommand,
    LegacyDatagramBuilder, LegacyError, PayloadError, RX_FRAME_BYTES, Segment, TX_FRAME_BYTES,
};
use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::error::EncodeError;
use autd3_rs_core::geometry::{Autd3, Device, Geometry, Point3, UnitQuaternion};
use autd3_rs_core::link::{DeviceState, StateCheck};
use autd3_rs_core::units::mm;
use autd3_rs_core::value::{
    ControlPoint, ControlPoints, DcSysTime, Emission, Intensity, LoopBehavior, ModulationBank,
    PatternBank, Phase, PulseWidth, SamplingConfig, TransitionMode,
};
use autd3_rs_pattern::{FocusOption, focus};

fn geometry(n: usize) -> Geometry {
    Geometry::new(
        (0..n)
            .map(|i| {
                Autd3::new(
                    Point3::new(200.0 * i as f32, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    )
}

fn link(geometry: &Geometry) -> (LegacyAudit, Vec<LegacyDeviceHandle>) {
    let link = LegacyAudit::new(geometry.iter().map(Device::num_transducers));
    let devices = link.devices();
    (link, devices)
}

async fn open(geometry: &Geometry) -> (LegacyClient, Vec<LegacyDeviceHandle>) {
    let (link, devices) = link(geometry);
    let client = LegacyClient::open(geometry, link, LegacyClientConfig::default())
        .await
        .expect("the emulator reports v12.1.0 firmware");
    (client, devices)
}

fn divide(v: u16) -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(v).unwrap())
}

fn pattern(geometry: &Geometry, target: Point3<f32>) -> Vec<Vec<Emission>> {
    let mut buf = geometry.pattern_buffer();
    focus(
        geometry,
        target,
        8.5 * mm,
        &FocusOption::default(),
        &mut buf,
    );
    buf
}

async fn send<'a, C: LegacyCommand<'a>>(client: &LegacyClient, cmd: C) -> Result<(), LegacyError> {
    let frames = {
        let mut builder = client.datagram_builder();
        builder.push(cmd);
        builder.build()?
    };
    for frame in &frames {
        client.send_checked(frame).await?;
    }
    Ok(())
}

async fn relax_silencer(client: &LegacyClient) {
    send(client, SetSilencer::disable()).await.unwrap();
}

#[tokio::test]
async fn open_clears_synchronizes_and_reads_the_firmware_version() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    assert_eq!(client.num_devices(), 2);
    let versions = client.firmware_version();
    assert_eq!(versions.len(), 2);
    for (idx, version) in versions.iter().enumerate() {
        assert_eq!(version.idx, idx);
        assert_eq!(version.cpu.to_string(), "legacy-v12.1.0");
        assert!(version.is_supported());
    }
    for device in &devices {
        assert!(device.with(LegacyDevice::synchronized));
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn open_with_checker_initializes_and_reports_the_link_status() {
    let geo = geometry(2);
    let (l, devices) = link(&geo);
    let (client, mut checker) =
        LegacyClient::open_with_checker(&geo, l, LegacyClientConfig::default())
            .await
            .expect("the emulator reports v12.1.0 firmware");

    assert_eq!(client.firmware_version().len(), 2);
    for device in &devices {
        assert!(device.with(LegacyDevice::synchronized));
    }

    let status = checker.check().await.unwrap();
    assert_eq!(status.devices, vec![DeviceState::Op; 2]);
    assert!(status.all_op());
    assert!(!status.any_lost());
    assert_eq!(status.recoveries, 0);

    client.close().await.unwrap();
}

#[tokio::test]
async fn open_rejects_firmware_older_than_v12_1() {
    let geo = geometry(1);
    let (l, devices) = link(&geo);
    devices[0].with_mut(|d| d.set_cpu_version(0xA4, 0x00));

    let err = LegacyClient::open(&geo, l, LegacyClientConfig::default())
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            LegacyError::UnsupportedFirmware { device: 0, version } if version == "legacy-v12.0.0"
        ),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn open_rejects_a_geometry_that_does_not_match_the_link() {
    let geo = geometry(2);
    let l = LegacyAudit::new([geo[0].num_transducers()]);
    let err = LegacyClient::open(&geo, l, LegacyClientConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LegacyError::DeviceCountMismatch {
            geometry: 2,
            link: 1
        }
    ));
}

#[tokio::test]
async fn the_current_pattern_command_reaches_the_device_unchanged() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let emissions = pattern(&geo, Point3::new(100.0, 60.0, 150.0));
    send(&client, Pattern::new(&emissions)).await.unwrap();

    for device in &geo {
        devices[device.idx()].with(|d| {
            let state = d.segment(Segment::S0);
            assert_eq!(state.kind, StmKind::Gain);
            assert_eq!(state.emissions.len(), 1);
            assert_eq!(state.emissions[0], emissions[device.idx()]);
        });
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn push_each_routes_a_different_pattern_to_each_device() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let left = pattern(&geo, Point3::new(-40.0, 60.0, 150.0));
    let right = pattern(&geo, Point3::new(240.0, 60.0, 150.0));
    assert_ne!(left[1], right[1], "the two targets must differ per device");

    let frames = {
        let mut builder = client.datagram_builder();
        builder
            .push_each(|device| Some(Pattern::new(if device.idx() == 0 { &left } else { &right })));
        builder.build().unwrap()
    };
    assert_eq!(frames.len(), 1, "both devices fit in a single round");
    for frame in &frames {
        client.send_checked(frame).await.unwrap();
    }

    devices[0].with(|d| assert_eq!(d.segment(Segment::S0).emissions[0], left[0]));
    devices[1].with(|d| assert_eq!(d.segment(Segment::S0).emissions[0], right[1]));
    client.close().await.unwrap();
}

#[tokio::test]
async fn push_each_leaves_unassigned_devices_untouched() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let send_each = async |value: bool, target: usize| {
        let frames = {
            let mut builder = client.datagram_builder();
            builder.push_each(|device| (device.idx() == target).then_some(ForceFan { value }));
            builder.build().unwrap()
        };
        assert_eq!(frames.len(), 1);
        for frame in &frames {
            client.send_checked(frame).await.unwrap();
        }
    };

    send_each(true, 0).await;
    assert!(devices[0].with(LegacyDevice::force_fan));
    assert!(!devices[1].with(LegacyDevice::force_fan));

    send_each(true, 1).await;
    assert!(devices[0].with(LegacyDevice::force_fan));
    assert!(devices[1].with(LegacyDevice::force_fan));

    client.close().await.unwrap();
}

#[tokio::test]
async fn pattern_bank_b1_maps_onto_legacy_segment_s1() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let emissions = pattern(&geo, Point3::new(100.0, 60.0, 150.0));
    send(&client, Pattern::with_bank(PatternBank::B1, &emissions))
        .await
        .unwrap();

    devices[0].with(|d| {
        assert_eq!(d.segment(Segment::S1).emissions[0], emissions[0]);
        assert_eq!(d.current_stm_segment(), Segment::S1);
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_current_modulation_command_is_reassembled_across_frames() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = (0..3000usize)
        .map(|i| u8::try_from(i % 256).expect("i % 256 fits in u8"))
        .collect::<Vec<_>>();
    send(&client, Modulation::new(divide(10), &buffer))
        .await
        .unwrap();

    devices[0].with(|d| {
        let state = d.segment(Segment::S0);
        assert_eq!(state.modulation, buffer);
        assert_eq!(state.mod_freq_div, 10);
        assert_eq!(state.mod_rep, 0xFFFF);
        assert_eq!(d.current_mod_segment(), Segment::S0);
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_current_foci_stm_command_is_reassembled_across_frames() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let points = (0..300)
        .map(|i| {
            ControlPoints::new(
                [ControlPoint::new(
                    Point3::new(100.0, 60.0, 140.0 + i as f32 * 0.1),
                    Phase::ZERO,
                )],
                Intensity(0x90),
            )
        })
        .collect::<Vec<_>>();

    send(
        &client,
        FociStm::new(divide(0xFFFF), &points, FociStmOption::default()),
    )
    .await
    .unwrap();

    for device in &geo {
        devices[device.idx()].with(|d| {
            let state = d.segment(Segment::S0);
            assert_eq!(state.kind, StmKind::Foci);
            assert_eq!(d.num_foci(), 1);
            assert_eq!(state.sound_speed, 340 * 64);
            assert_eq!(state.foci.len(), points.len());
            for (i, encoded) in state.foci.iter().enumerate() {
                assert_eq!(*encoded, points[i].focus(device, 0).encode().unwrap());
            }
        });
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn multi_foci_stm_is_reassembled_across_frames() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let points = (0..120)
        .map(|i| {
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(90.0, 60.0, 140.0 + i as f32), Phase::ZERO),
                    ControlPoint::new(Point3::new(110.0, 60.0, 140.0 + i as f32), Phase(0x40)),
                ],
                Intensity(0x80),
            )
        })
        .collect::<Vec<_>>();

    send(
        &client,
        FociStm::new(divide(0xFFFF), &points, FociStmOption::default()),
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        let state = d.segment(Segment::S0);
        assert_eq!(d.num_foci(), 2);
        assert_eq!(state.foci.len(), points.len() * 2);
        for (i, points) in points.iter().enumerate() {
            for j in 0..2 {
                assert_eq!(
                    state.foci[i * 2 + j],
                    points.focus(&geo[0], j).encode().unwrap()
                );
            }
        }
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_current_pattern_stm_command_is_reassembled_across_frames() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let patterns = (0..8)
        .map(|i| pattern(&geo, Point3::new(100.0, 60.0, 140.0 + i as f32)))
        .collect::<Vec<_>>();

    send(
        &client,
        PatternStm::new(divide(0xFFFF), &patterns, PatternStmOption::default()),
    )
    .await
    .unwrap();

    for device in &geo {
        devices[device.idx()].with(|d| {
            let state = d.segment(Segment::S0);
            assert_eq!(state.kind, StmKind::Gain);
            assert_eq!(state.emissions.len(), patterns.len());
            for (i, emissions) in state.emissions.iter().enumerate() {
                assert_eq!(*emissions, patterns[i][device.idx()]);
            }
        });
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn pattern_stm_phase_full_keeps_only_the_phase() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let patterns = (0..6)
        .map(|i| pattern(&geo, Point3::new(80.0, 60.0, 140.0 + i as f32)))
        .collect::<Vec<_>>();

    send(
        &client,
        PatternStm::new(
            divide(0xFFFF),
            &patterns,
            PatternStmOption {
                mode: PatternStmMode::PhaseFull,
                ..PatternStmOption::default()
            },
        ),
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        let state = d.segment(Segment::S0);
        assert_eq!(state.emissions.len(), patterns.len());
        for (i, emissions) in state.emissions.iter().enumerate() {
            for (tr, emission) in emissions.iter().enumerate() {
                assert_eq!(emission.phase, patterns[i][0][tr].phase);
                assert_eq!(emission.intensity, Intensity::MAX);
            }
        }
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn pattern_stm_phase_half_keeps_only_the_upper_nibble() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let patterns = (0..4)
        .map(|i| pattern(&geo, Point3::new(80.0, 60.0, 140.0 + i as f32)))
        .collect::<Vec<_>>();

    send(
        &client,
        PatternStm::new(
            divide(0xFFFF),
            &patterns,
            PatternStmOption {
                mode: PatternStmMode::PhaseHalf,
                ..PatternStmOption::default()
            },
        ),
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        let state = d.segment(Segment::S0);
        assert_eq!(state.emissions.len(), 4);
        for (i, emissions) in state.emissions.iter().enumerate() {
            for (tr, emission) in emissions.iter().enumerate() {
                let nibble = patterns[i][0][tr].phase.0 >> 4;
                assert_eq!(emission.phase, Phase(nibble << 4 | nibble));
                assert_eq!(emission.intensity, Intensity::MAX);
            }
        }
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn several_commands_share_one_builder() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let emissions = pattern(&geo, Point3::new(100.0, 60.0, 150.0));
    let buffer = vec![0x80u8; 100];

    let frames = {
        let mut builder = client.datagram_builder();
        builder
            .push(SetSilencer::disable())
            .push(Pattern::new(&emissions))
            .push(Modulation::new(divide(10), &buffer));
        builder.build().unwrap()
    };
    assert_eq!(frames.len(), 3, "one operation per frame");
    for frame in &frames {
        client.send_checked(frame).await.unwrap();
    }

    devices[0].with(|d| {
        assert!(!d.silencer_strict());
        assert_eq!(d.segment(Segment::S0).emissions[0], emissions[0]);
        assert_eq!(d.segment(Segment::S0).modulation, buffer);
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_modulation_bank_switch_activates_the_other_bank() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x40u8; 8];
    send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Ext,
            ..Modulation::new(divide(10), &buffer)
        },
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.segment(Segment::S1).modulation, buffer));

    send(
        &client,
        ChangeModulationBank {
            bank: ModulationBank::B0,
            transition_mode: TransitionMode::Ext,
        },
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_mod_segment(), Segment::S0));
    client.close().await.unwrap();
}

#[tokio::test]
async fn later_stages_a_modulation_bank_without_activating_it() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x40u8; 8];
    send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Later,
            ..Modulation::new(divide(10), &buffer)
        },
    )
    .await
    .unwrap();
    devices[0].with(|d| {
        assert_eq!(d.segment(Segment::S1).modulation, buffer);
        assert_eq!(d.current_mod_segment(), Segment::S0, "still on B0");
    });

    send(
        &client,
        ChangeModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Immediate,
        },
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_mod_segment(), Segment::S1));
    client.close().await.unwrap();
}

#[tokio::test]
async fn later_stages_an_stm_bank_without_activating_it() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let points = (0..2)
        .map(|i| ControlPoints::from(ControlPoint::from(Point3::new(0.0, 0.0, 140.0 + i as f32))))
        .collect::<Vec<_>>();
    send(
        &client,
        FociStm::new(
            divide(0xFFFF),
            &points,
            FociStmOption {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::Later,
                ..Default::default()
            },
        ),
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S0, "still on B0"));

    send(
        &client,
        LegacyChangePatternBank::foci_stm(PatternBank::B1, TransitionMode::Immediate),
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S1));
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_bank_change_refuses_to_not_transition() {
    let geo = geometry(1);
    let (client, _devices) = open(&geo).await;

    let err = send(
        &client,
        ChangeModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Later,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Encode(EncodeError::TransitionLaterNotEncodable)
        ),
        "unexpected error: {err}"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn an_infinite_loop_cannot_transition_on_a_sampling_index() {
    let geo = geometry(1);
    let (client, _devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x80u8; 4];
    let err = send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::SyncIdx,
            ..Modulation::new(divide(10), &buffer)
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: INVALID_TRANSITION_MODE
            }
        ),
        "unexpected error: {err}"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_transition_time_in_the_past_is_rejected_by_the_device() {
    let geo = geometry(1);
    let (client, _devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x80u8; 4];
    let err = send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            loop_behavior: LoopBehavior::ONCE,
            transition_mode: TransitionMode::SysTime {
                time: DcSysTime::ZERO,
                margin: None,
            },
            ..Modulation::new(divide(10), &buffer)
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: MISS_TRANSITION_TIME
            }
        ),
        "unexpected error: {err}"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_strict_silencer_rejects_a_too_fast_modulation() {
    let geo = geometry(1);
    let (client, _devices) = open(&geo).await;

    let buffer = vec![0x80u8; 4];
    let err = send(&client, Modulation::new(divide(1), &buffer))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: INVALID_SILENCER_SETTINGS
            }
        ),
        "unexpected error: {err}"
    );
    client.close().await.unwrap();
}

#[tokio::test]
async fn fixed_update_rate_silencer_releases_strict_mode() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    send(
        &client,
        SetSilencer::new(FixedUpdateRate {
            intensity: NonZeroU16::new(0x0100).unwrap(),
            phase: NonZeroU16::new(0x0200).unwrap(),
        }),
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        assert!(d.silencer_fixed_update_rate());
        assert!(!d.silencer_strict());
        assert_eq!(d.silencer_update_rate(), (0x0100, 0x0200));
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn completion_time_silencer_converts_to_ultrasound_periods() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    send(
        &client,
        SetSilencer::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 20,
            phase: ULTRASOUND_PERIOD * 60,
            strict_mode: true,
        }),
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        assert!(d.silencer_strict());
        assert_eq!(d.silencer_completion_steps(), (20, 60));
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn read_fpga_state_enables_the_rx_byte_on_first_use() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    assert!(!devices[0].with(LegacyDevice::reads_fpga_state));
    devices[1].with_mut(|d| d.set_thermal_assert(true));

    let states = client.read_fpga_state().await.unwrap();
    assert_eq!(states.len(), 2);
    assert!(states.iter().all(|s| s.is_valid()));
    assert!(!states[0].is_thermal_assert());
    assert!(states[1].is_thermal_assert());
    assert!(devices[0].with(LegacyDevice::reads_fpga_state));

    client.close().await.unwrap();
}

#[tokio::test]
async fn stop_writes_a_null_pattern() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    send(
        &client,
        Pattern::new(&pattern(&geo, Point3::new(0.0, 0.0, 150.0))),
    )
    .await
    .unwrap();
    client.stop().await.unwrap();

    devices[0].with(|d| {
        assert!(
            d.segment(Segment::S0).emissions[0]
                .iter()
                .all(|e| *e == Emission::NULL)
        );
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn force_fan_is_latched_on_the_device() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    assert!(!devices[0].with(LegacyDevice::force_fan));
    send(&client, ForceFan { value: true }).await.unwrap();
    assert!(devices[0].with(LegacyDevice::force_fan));
    send(&client, ForceFan { value: false }).await.unwrap();
    assert!(!devices[0].with(LegacyDevice::force_fan));
    client.close().await.unwrap();
}

#[tokio::test]
async fn each_command_occupies_its_own_frame() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let frames = {
        let mut builder = client.datagram_builder();
        builder.push(Clear).push(Synchronize);
        builder.build().unwrap()
    };
    assert_eq!(frames.len(), 2, "one command per frame");
    assert_eq!(frames.frame(0).unwrap().num_devices(), 1);

    for frame in &frames {
        client.send_checked(frame).await.unwrap();
    }
    devices[0].with(|d| assert!(d.synchronized()));
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_legacy_only_change_segment_command_is_still_available() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let emissions = pattern(&geo, Point3::new(100.0, 60.0, 150.0));
    send(
        &client,
        op::Gain::with_segment(&emissions, Segment::S1, false),
    )
    .await
    .unwrap();
    devices[0].with(|d| {
        assert_eq!(d.segment(Segment::S1).emissions[0], emissions[0]);
        assert_eq!(
            d.current_stm_segment(),
            Segment::S1,
            "a legacy gain write moves the segment pointer even without the UPDATE flag"
        );
    });

    send(&client, LegacyChangePatternBank::pattern(PatternBank::B0))
        .await
        .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S0));
    send(&client, LegacyChangePatternBank::pattern(PatternBank::B1))
        .await
        .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S1));
    client.close().await.unwrap();
}

#[tokio::test]
async fn change_segment_switches_back_to_an_stm_bank() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let points = (0..2)
        .map(|i| ControlPoints::from(ControlPoint::from(Point3::new(0.0, 0.0, 140.0 + i as f32))))
        .collect::<Vec<_>>();
    for bank in [PatternBank::B0, PatternBank::B1] {
        send(
            &client,
            FociStm::new(
                divide(0xFFFF),
                &points,
                FociStmOption {
                    bank,
                    ..Default::default()
                },
            ),
        )
        .await
        .unwrap();
    }
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S1));

    send(
        &client,
        LegacyChangePatternBank::foci_stm(PatternBank::B0, TransitionMode::Immediate),
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S0));

    let patterns = (0..2)
        .map(|i| pattern(&geo, Point3::new(0.0, 0.0, 140.0 + i as f32)))
        .collect::<Vec<_>>();
    for bank in [PatternBank::B0, PatternBank::B1] {
        send(
            &client,
            PatternStm::new(
                divide(0xFFFF),
                &patterns,
                PatternStmOption {
                    bank,
                    ..Default::default()
                },
            ),
        )
        .await
        .unwrap();
    }
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S1));

    send(
        &client,
        LegacyChangePatternBank::pattern_stm(PatternBank::B0, TransitionMode::Immediate),
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S0));
    client.close().await.unwrap();
}

#[tokio::test]
async fn close_relaxes_the_silencer_stops_output_and_clears() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    send(
        &client,
        Pattern::new(&pattern(&geo, Point3::new(0.0, 0.0, 150.0))),
    )
    .await
    .unwrap();
    client.close().await.unwrap();

    devices[0].with(|d| {
        assert!(!d.reads_fpga_state());
        assert!(d.silencer_strict(), "Clear restores the boot silencer");
        assert!(
            d.segment(Segment::S0).emissions[0]
                .iter()
                .all(|e| *e == Emission::NULL)
        );
    });
}

#[tokio::test]
async fn open_must_not_trust_an_ack_left_by_an_earlier_session() {
    let geo = geometry(1);
    let (l, devices) = link(&geo);
    devices[0].with_mut(|d| {
        d.set_ack_state(0, INVALID_TRANSITION_MODE);
        d.set_last_msg_id(0);
    });

    let client = LegacyClient::open(&geo, l, LegacyClientConfig::default())
        .await
        .expect("the stale error belongs to a frame this session never sent");
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_priming_handshake_avoids_every_stale_ack() {
    let geo = geometry(4);
    let (l, devices) = link(&geo);
    for (i, device) in devices.iter().enumerate() {
        device.with_mut(|d| {
            d.set_ack_state(u8::try_from(i).unwrap(), INVALID_TRANSITION_MODE);
            d.set_last_msg_id(u8::try_from(i).unwrap());
        });
    }

    let client = LegacyClient::open(&geo, l, LegacyClientConfig::default())
        .await
        .expect("a free message id exists");
    client.close().await.unwrap();
}

#[tokio::test]
async fn output_mask_reaches_the_device_bit_packed() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let masks = geo
        .iter()
        .map(|d| (0..d.num_transducers()).map(|i| i % 3 == 0).collect())
        .collect::<Vec<Vec<bool>>>();
    send(&client, SetOutputMask { masks: &masks })
        .await
        .unwrap();

    for device in &geo {
        devices[device.idx()].with(|d| {
            assert_eq!(d.output_mask(Segment::S0), masks[device.idx()].as_slice());
        });
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn phase_correction_reaches_the_device() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let phases = geo
        .iter()
        .map(|d| {
            (0..d.num_transducers())
                .map(|i| Phase(u8::try_from(i % 256).unwrap()))
                .collect()
        })
        .collect::<Vec<Vec<Phase>>>();
    send(&client, SetPhaseCorrection { phases: &phases })
        .await
        .unwrap();

    for device in &geo {
        devices[device.idx()].with(|d| {
            assert_eq!(d.phase_correction(), phases[device.idx()].as_slice());
        });
    }
    client.close().await.unwrap();
}

#[tokio::test]
async fn the_pulse_width_table_reaches_the_device_as_256_words() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let table = SetPulseWidthTable::default_table();
    send(&client, SetPulseWidthTable { table: &table })
        .await
        .unwrap();

    devices[0].with(|d| {
        let actual = d.pulse_width_table();
        assert_eq!(actual.len(), 256);
        for (i, entry) in table.iter().enumerate() {
            assert_eq!(actual[i], entry.pulse_width().unwrap());
        }
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_custom_pulse_width_table_is_carried_verbatim() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let table: [PulseWidth; 256] =
        core::array::from_fn(|i| PulseWidth::new(u16::try_from(i).unwrap()));
    send(&client, SetPulseWidthTable { table: &table })
        .await
        .unwrap();

    devices[0].with(|d| {
        for i in 0..256 {
            assert_eq!(d.pulse_width_table()[i], u16::try_from(i).unwrap());
        }
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn gpio_out_reaches_the_device_with_legacy_type_tags() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let time = DcSysTime::from_nanos(3125 * 4096);
    send(
        &client,
        SetGpioOut {
            outputs: [
                GpioOut::BaseSignal,
                GpioOut::PatternIdx(0x0123),
                GpioOut::PwmOut(7),
                GpioOut::SysTimeEq(time),
            ],
        },
    )
    .await
    .unwrap();

    devices[0].with(|d| {
        let out = d.gpio_out();
        assert_eq!(out[0] >> 56, 0x01, "BaseSignal");
        assert_eq!(out[1] >> 56, 0x51, "PatternIdx maps onto StmIdx");
        assert_eq!(out[1] & 0x00FF_FFFF_FFFF_FFFF, 0x0123);
        assert_eq!(out[2] >> 56, 0xE0, "PwmOut");
        assert_eq!(out[2] & 0x00FF_FFFF_FFFF_FFFF, 7);
        assert_eq!(out[3] >> 56, 0x60, "SysTimeEq");
        assert_eq!(
            out[3] & 0x00FF_FFFF_FFFF_FFFF,
            ((time.sys_time() / 3125) << 6) >> 9
        );
    });
    client.close().await.unwrap();
}

#[tokio::test]
async fn emulated_gpio_in_reaches_the_device() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let values = [true, false, true, true];
    send(&client, EmulateGpioIn { values }).await.unwrap();
    devices[0].with(|d| assert_eq!(d.gpio_in(), values));

    send(&client, EmulateGpioIn { values: [false; 4] })
        .await
        .unwrap();
    devices[0].with(|d| assert_eq!(d.gpio_in(), [false; 4]));
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_priming_id_swallowed_by_de_duplication_is_retried() {
    let geo = geometry(1);
    let (l, devices) = link(&geo);
    devices[0].with_mut(|d| {
        d.set_ack_state(3, 0);
        d.set_last_msg_id(1);
    });

    let client = LegacyClient::open(&geo, l, LegacyClientConfig::default())
        .await
        .expect("the handshake tries another id when a prime is swallowed");
    client.close().await.unwrap();
}

#[tokio::test]
async fn an_unresponsive_device_is_reported_as_a_timeout_with_its_last_ack() {
    let geo = geometry(1);
    let (l, devices) = link(&geo);
    devices[0].with_mut(|d| {
        d.set_ack_state(7, INVALID_TRANSITION_MODE);
        d.wedge();
    });

    let err = LegacyClient::open(
        &geo,
        l,
        LegacyClientConfig {
            timeout_cycles: NonZeroU32::new(40).unwrap(),
        },
    )
    .await
    .unwrap_err();

    let message = err.to_string();
    assert!(
        matches!(err, LegacyError::Timeout { phase, .. } if phase == TimeoutPhase::Handshake),
        "unexpected error: {message}"
    );
    assert!(
        message.contains("msg_id=0x07") && message.contains("err=0x08"),
        "the timeout should surface what the device is still reporting: {message}"
    );
}

const TAG_GAIN_STM: u8 = 0x41;
const GAIN_STM_FLAG_BEGIN: u8 = 1 << 0;
const GAIN_STM_FLAG_END: u8 = 1 << 1;
const GAIN_STM_TRANSITION_MODE_NONE: u8 = 0xFE;
const GAIN_STM_FREQ_DIV: u16 = 0x1000;
const GAIN_STM_WORD: [u8; 2] = [0xAB, 0xCD];

fn gain_stm_frame(msg_id: u8, mode: u8, send: u8, num_transducers: usize) -> [u8; TX_FRAME_BYTES] {
    let mut tx = [0u8; TX_FRAME_BYTES];
    tx[0] = msg_id;
    let payload = &mut tx[4..];
    payload[0] = TAG_GAIN_STM;
    payload[1] = GAIN_STM_FLAG_BEGIN | GAIN_STM_FLAG_END | ((send - 1) << 6);
    payload[2] = mode;
    payload[3] = GAIN_STM_TRANSITION_MODE_NONE;
    payload[4..6].copy_from_slice(&GAIN_STM_FREQ_DIV.to_le_bytes());
    payload[6..8].copy_from_slice(&0xFFFFu16.to_le_bytes());
    for chunk in payload[16..16 + num_transducers * 2].chunks_exact_mut(2) {
        chunk.copy_from_slice(&GAIN_STM_WORD);
    }
    tx
}

fn run_gain_stm_frame(mode: u8, send: u8) -> LegacyDevice {
    let num_transducers = 249;
    let mut device = LegacyDevice::new(0, num_transducers);
    let tx = gain_stm_frame(1, mode, send, num_transducers);
    let mut rx = [0u8; RX_FRAME_BYTES];
    device.cycle(&tx, &mut rx);
    device
}

#[tokio::test]
async fn a_bank_change_rechecks_the_silencer_against_the_target_bank() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x40u8; 8];
    send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Later,
            ..Modulation::new(divide(1), &buffer)
        },
    )
    .await
    .unwrap();

    send(
        &client,
        SetSilencer::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 10,
            phase: ULTRASOUND_PERIOD * 40,
            strict_mode: true,
        }),
    )
    .await
    .unwrap();

    let err = send(
        &client,
        ChangeModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Immediate,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: INVALID_SILENCER_SETTINGS
            }
        ),
        "unexpected error: {err}"
    );
    devices[0].with(|d| assert_eq!(d.current_mod_segment(), Segment::S0));
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_pattern_write_leaves_the_stm_kind_on_the_previous_sequence() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let points = (0..2)
        .map(|i| ControlPoints::from(ControlPoint::from(Point3::new(0.0, 0.0, 140.0 + i as f32))))
        .collect::<Vec<_>>();
    send(
        &client,
        FociStm::new(divide(0xFFFF), &points, FociStmOption::default()),
    )
    .await
    .unwrap();

    send(
        &client,
        Pattern::new(&pattern(&geo, Point3::new(0.0, 0.0, 150.0))),
    )
    .await
    .unwrap();
    devices[0].with(|d| {
        let state = d.segment(Segment::S0);
        assert_eq!(state.kind, StmKind::Foci, "only an STM END moves the kind");
        assert_eq!(state.cycle, 1);
    });

    let err = send(&client, LegacyChangePatternBank::pattern(PatternBank::B0))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: INVALID_SEGMENT_TRANSITION
            }
        ),
        "unexpected error: {err}"
    );

    send(
        &client,
        LegacyChangePatternBank::foci_stm(PatternBank::B0, TransitionMode::Immediate),
    )
    .await
    .unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_single_slot_sequence_reports_gain_mode_whatever_the_stm_kind() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    let points = (0..2)
        .map(|i| ControlPoints::from(ControlPoint::from(Point3::new(0.0, 0.0, 140.0 + i as f32))))
        .collect::<Vec<_>>();
    send(
        &client,
        FociStm::new(divide(0xFFFF), &points, FociStmOption::default()),
    )
    .await
    .unwrap();
    assert!(!client.read_fpga_state().await.unwrap()[0].is_gain_mode());

    send(
        &client,
        Pattern::new(&pattern(&geo, Point3::new(0.0, 0.0, 150.0))),
    )
    .await
    .unwrap();
    devices[0].with(|d| assert_eq!(d.segment(Segment::S0).kind, StmKind::Foci));
    assert!(client.read_fpga_state().await.unwrap()[0].is_gain_mode());

    client.close().await.unwrap();
}

#[tokio::test]
async fn a_missed_transition_time_still_moves_the_bank() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x40u8; 8];
    send(
        &client,
        Modulation {
            bank: ModulationBank::B1,
            loop_behavior: LoopBehavior::ONCE,
            transition_mode: TransitionMode::Later,
            ..Modulation::new(divide(10), &buffer)
        },
    )
    .await
    .unwrap();

    let err = send(
        &client,
        ChangeModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::SysTime {
                time: DcSysTime::ZERO,
                margin: None,
            },
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: MISS_TRANSITION_TIME
            }
        ),
        "unexpected error: {err}"
    );
    devices[0].with(|d| assert_eq!(d.current_mod_segment(), Segment::S1));
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_missed_stm_transition_time_still_moves_the_bank() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let points = (0..2)
        .map(|i| ControlPoints::from(ControlPoint::from(Point3::new(0.0, 0.0, 140.0 + i as f32))))
        .collect::<Vec<_>>();
    send(
        &client,
        FociStm::new(
            divide(0xFFFF),
            &points,
            FociStmOption {
                bank: PatternBank::B1,
                loop_behavior: LoopBehavior::ONCE,
                transition_mode: TransitionMode::Later,
                ..Default::default()
            },
        ),
    )
    .await
    .unwrap();

    let err = send(
        &client,
        LegacyChangePatternBank::foci_stm(
            PatternBank::B1,
            TransitionMode::SysTime {
                time: DcSysTime::ZERO,
                margin: None,
            },
        ),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 0,
                code: MISS_TRANSITION_TIME
            }
        ),
        "unexpected error: {err}"
    );
    devices[0].with(|d| assert_eq!(d.current_stm_segment(), Segment::S1));
    client.close().await.unwrap();
}

#[tokio::test]
async fn clear_restores_the_masks_corrections_table_and_gpio() {
    let geo = geometry(1);
    let (client, devices) = open(&geo).await;

    devices[0].with(|d| {
        assert_eq!(d.pulse_width_table(), default_pulse_width_table());
    });

    let masks = geo
        .iter()
        .map(|d| (0..d.num_transducers()).map(|i| i % 3 == 0).collect())
        .collect::<Vec<Vec<bool>>>();
    let phases = geo
        .iter()
        .map(|d| (0..d.num_transducers()).map(|_| Phase(0x7F)).collect())
        .collect::<Vec<Vec<Phase>>>();
    let table: [PulseWidth; 256] =
        core::array::from_fn(|i| PulseWidth::new(u16::try_from(i).unwrap()));

    send(&client, SetOutputMask { masks: &masks })
        .await
        .unwrap();
    send(&client, SetPhaseCorrection { phases: &phases })
        .await
        .unwrap();
    send(&client, SetPulseWidthTable { table: &table })
        .await
        .unwrap();
    send(
        &client,
        SetGpioOut {
            outputs: [
                GpioOut::BaseSignal,
                GpioOut::BaseSignal,
                GpioOut::BaseSignal,
                GpioOut::BaseSignal,
            ],
        },
    )
    .await
    .unwrap();
    send(&client, EmulateGpioIn { values: [true; 4] })
        .await
        .unwrap();

    send(&client, Clear).await.unwrap();

    devices[0].with(|d| {
        assert!(d.output_mask(Segment::S0).iter().all(|&v| v));
        assert!(d.output_mask(Segment::S1).iter().all(|&v| v));
        assert!(d.phase_correction().iter().all(|&p| p == Phase::ZERO));
        assert_eq!(d.pulse_width_table(), default_pulse_width_table());
        assert_eq!(d.gpio_out(), [0; 4]);
        assert_eq!(d.gpio_in(), [false; 4]);
    });
    client.close().await.unwrap();
}

#[test]
fn gain_stm_phase_intensity_full_ignores_the_send_count() {
    let device = run_gain_stm_frame(0, 4);

    assert_eq!(device.err(), 0);
    let state = device.segment(Segment::S0);
    assert_eq!(state.cycle, 1);
    assert_eq!(state.emissions.len(), 1);
    assert!(state.emissions[0].iter().all(|e| *e
        == Emission {
            phase: Phase(GAIN_STM_WORD[0]),
            intensity: Intensity(GAIN_STM_WORD[1]),
        }));
}

#[test]
fn gain_stm_phase_full_drops_an_out_of_range_send_count() {
    let device = run_gain_stm_frame(1, 4);

    assert_eq!(device.err(), 0);
    let state = device.segment(Segment::S0);
    assert_eq!(state.cycle, 2);
    assert_eq!(state.emissions.len(), 2);
    assert!(state.emissions[0].iter().all(|e| *e
        == Emission {
            phase: Phase(GAIN_STM_WORD[0]),
            intensity: Intensity::MAX,
        }));
    assert!(state.emissions[1].iter().all(|e| *e
        == Emission {
            phase: Phase(GAIN_STM_WORD[1]),
            intensity: Intensity::MAX,
        }));
}

#[test]
fn an_invalid_gain_stm_mode_is_rejected_after_the_head_is_applied() {
    let device = run_gain_stm_frame(3, 1);

    assert_eq!(device.err(), INVALID_GAIN_STM_MODE);
    assert_eq!(device.gain_stm_mode(), 3);
    let state = device.segment(Segment::S0);
    assert_eq!(state.freq_div, GAIN_STM_FREQ_DIV);
    assert_eq!(state.cycle, 0);
    assert!(state.emissions.is_empty());
}

async fn send_foreign_frames(client: &LegacyClient, devices: usize) -> Result<(), LegacyError> {
    let foreign = Arc::new(geometry(devices));
    let mut builder = LegacyDatagramBuilder::new(foreign);
    builder.push(Clear);
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_frame_built_for_more_devices_is_rejected_before_it_reaches_the_bus() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    let err = send_foreign_frames(&client, 3).await.unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::FrameDeviceCountMismatch {
                expected: 2,
                got: 3
            })
        ),
        "unexpected error: {err}"
    );
    for device in &devices {
        assert!(device.with(LegacyDevice::synchronized));
    }

    send(&client, Clear).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_frame_built_for_fewer_devices_is_rejected_instead_of_timing_out() {
    let geo = geometry(2);
    let (client, _devices) = open(&geo).await;

    let err = send_foreign_frames(&client, 1).await.unwrap_err();
    assert!(
        matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::FrameDeviceCountMismatch {
                expected: 2,
                got: 1
            })
        ),
        "unexpected error: {err}"
    );

    send(&client, Clear).await.unwrap();
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_device_that_stops_answering_after_open_reports_a_command_timeout() {
    const TAG_FORCE_FAN: u8 = 0x60;

    let geo = geometry(1);
    let (l, devices) = link(&geo);
    let client = LegacyClient::open(
        &geo,
        l,
        LegacyClientConfig {
            timeout_cycles: NonZeroU32::new(40).unwrap(),
        },
    )
    .await
    .expect("the emulator reports v12.1.0 firmware");

    devices[0].with_mut(LegacyDevice::wedge);

    let err = send(&client, ForceFan { value: true }).await.unwrap_err();
    let message = err.to_string();
    assert!(
        matches!(
            err,
            LegacyError::Timeout {
                phase: TimeoutPhase::Command { tag: TAG_FORCE_FAN },
                ..
            }
        ),
        "unexpected error: {message}"
    );
    assert!(
        message.contains("command 0x60"),
        "the timeout should name the command that stalled: {message}"
    );
}

#[tokio::test]
async fn an_error_is_attributed_to_the_device_that_reported_it() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = vec![0x80u8; 4];
    let frames = {
        let mut builder = client.datagram_builder();
        builder.push_each(|device| {
            (device.idx() == 1).then_some(Modulation {
                bank: ModulationBank::B1,
                loop_behavior: LoopBehavior::Infinite,
                transition_mode: TransitionMode::SyncIdx,
                ..Modulation::new(divide(10), &buffer)
            })
        });
        builder.build().unwrap()
    };
    let mut err = None;
    for frame in &frames {
        if let Err(e) = client.send_checked(frame).await {
            err = Some(e);
            break;
        }
    }
    let err = err.expect("device 1 rejects the transition mode");
    assert!(
        matches!(
            err,
            LegacyError::Device {
                device: 1,
                code: INVALID_TRANSITION_MODE
            }
        ),
        "unexpected error: {err}"
    );
    devices[0].with(|d| assert_eq!(d.err(), 0, "device 0 saw nothing to reject"));

    client.close().await.unwrap();
}

#[tokio::test]
async fn push_each_pads_the_device_with_fewer_frames() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;
    relax_silencer(&client).await;

    let buffer = (0..3000usize)
        .map(|i| u8::try_from(i % 256).expect("i % 256 fits in u8"))
        .collect::<Vec<_>>();
    let untouched = devices[1].with(|d| d.segment(Segment::S0).modulation.clone());
    let frames = {
        let mut builder = client.datagram_builder();
        builder
            .push_each(|device| (device.idx() == 0).then(|| Modulation::new(divide(10), &buffer)))
            .push_each(|device| (device.idx() == 1).then_some(ForceFan { value: true }));
        builder.build().unwrap()
    };
    assert!(
        frames.len() > 1,
        "the modulation must span more frames than the fan command"
    );
    for frame in &frames {
        client.send_checked(frame).await.unwrap();
    }

    devices[0].with(|d| {
        assert_eq!(d.segment(Segment::S0).modulation, buffer);
        assert!(!d.force_fan(), "device 0 was never sent the fan command");
    });
    devices[1].with(|d| {
        assert!(d.force_fan());
        assert_eq!(
            d.segment(Segment::S0).modulation,
            untouched,
            "the padding frames must be no-ops"
        );
        assert_eq!(d.err(), 0);
    });

    client.close().await.unwrap();
}

#[tokio::test]
async fn reading_the_firmware_version_restores_the_fpga_state_flag() {
    let geo = geometry(2);
    let (client, devices) = open(&geo).await;

    client.read_fpga_state().await.unwrap();
    for device in &devices {
        assert!(device.with(LegacyDevice::reads_fpga_state));
    }

    devices[1].with_mut(|d| d.set_cpu_version(0xA5, 0x07));
    let versions = client.read_firmware_version().await.unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[1].cpu.minor, 0x07);

    for device in &devices {
        assert!(
            device.with(LegacyDevice::reads_fpga_state),
            "the info request must restore the flag it borrowed"
        );
    }
    let states = client.read_fpga_state().await.unwrap();
    assert!(states.iter().all(|s| s.is_valid()));

    client.close().await.unwrap();
}

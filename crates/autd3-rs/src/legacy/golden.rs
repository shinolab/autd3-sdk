#![cfg(test)]

use core::num::NonZeroU16;
use core::time::Duration;
use std::collections::BTreeMap;

use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
use autd3_rs_core::value::{
    ControlPoint, ControlPoints, DcSysTime, Emission, GpioIn, Intensity, LoopBehavior, Phase,
    PulseWidth, SamplingConfig,
};
use std::sync::Arc;

use crate::legacy::datagram::{LegacyDatagramBuilder, LegacyFrames};
use crate::legacy::op::{
    Clear, EmulateGpioIn, FirmInfo, FociStm, FociStmOption, ForceFan, Gain, GainStm, GainStmOption,
    LegacyChangePatternBank, LegacyOperation, Modulation, ModulationOption, Nop, ReadsFpgaState,
    SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, Silencer, SilencerConfig,
    Sync,
};
use crate::legacy::wire::params::PWE_TABLE_SIZE;
use crate::legacy::wire::{
    GainStmMode, GpioOut, InfoType, Segment, TX_FRAME_BYTES, TransitionMode,
};

const GOLDEN: &str = include_str!("../../tests/golden/legacy_v38_pack.tsv");

type Golden = BTreeMap<String, Vec<Vec<[u8; TX_FRAME_BYTES]>>>;

fn golden() -> Golden {
    let mut map: Golden = BTreeMap::new();
    for line in GOLDEN.lines().filter(|l| !l.trim().is_empty()) {
        let mut fields = line.split('\t');
        let case = fields.next().expect("case name").to_owned();
        let round: usize = fields.next().expect("round").parse().expect("round index");
        let device: usize = fields
            .next()
            .expect("device")
            .parse()
            .expect("device index");
        let hex = fields.next().expect("hex payload");
        assert_eq!(hex.len(), TX_FRAME_BYTES * 2, "{case}: frame is 626 bytes");

        let mut frame = [0u8; TX_FRAME_BYTES];
        for (byte, chunk) in frame.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            *byte = u8::from_str_radix(core::str::from_utf8(chunk).expect("ascii hex"), 16)
                .expect("hex byte");
        }

        let rounds = map.entry(case).or_default();
        if rounds.len() <= round {
            rounds.resize(round + 1, Vec::new());
        }
        let devices = &mut rounds[round];
        if devices.len() <= device {
            devices.resize(device + 1, [0u8; TX_FRAME_BYTES]);
        }
        devices[device] = frame;
    }
    map
}

fn geometry() -> Geometry {
    Geometry::new(
        (0..2)
            .map(|i| {
                Autd3::new(
                    Point3::new(200.0 * i as f32, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    )
}

fn emission(i: usize) -> Emission {
    let i = u8::try_from(i % 256).expect("i % 256 fits in u8");
    Emission {
        phase: Phase(i),
        intensity: Intensity(255 - i),
    }
}

fn pattern(geometry: &Geometry, offset: usize) -> Vec<Vec<Emission>> {
    geometry
        .iter()
        .map(|d| {
            (0..d.num_transducers())
                .map(|i| emission(i + offset))
                .collect()
        })
        .collect()
}

fn divide(v: u16) -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(v).unwrap())
}

fn describe(a: &[u8; TX_FRAME_BYTES], b: &[u8; TX_FRAME_BYTES]) -> String {
    let first = a
        .iter()
        .zip(b.iter())
        .position(|(x, y)| x != y)
        .expect("called only on mismatch");
    let lo = first.saturating_sub(4);
    let hi = (first + 8).min(TX_FRAME_BYTES);
    format!(
        "first difference at byte {first}: expected {:02x?}, got {:02x?} (window {lo}..{hi})",
        &a[lo..hi],
        &b[lo..hi]
    )
}

fn assert_case<'a, O: LegacyOperation + Clone + 'a>(golden: &Golden, case: &str, op: O) {
    let mut builder = LegacyDatagramBuilder::new(Arc::new(geometry()));
    builder.push_op(op);
    let frames = builder
        .build()
        .unwrap_or_else(|e| panic!("{case}: encode failed: {e}"));
    assert_matches(golden, case, &frames);
}

fn assert_matches(golden: &Golden, case: &str, frames: &LegacyFrames) {
    let expected = golden
        .get(case)
        .unwrap_or_else(|| panic!("{case}: no golden data"));
    assert_eq!(
        frames.len(),
        expected.len(),
        "{case}: frame count differs from the legacy SDK"
    );
    for (round, expected) in expected.iter().enumerate() {
        let round_frame = frames.frame(round).expect("round exists");
        let actual = round_frame.frames();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{case} round {round}: devices"
        );
        for (device, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            let mut bytes = actual.to_bytes();
            bytes[0] = 0;
            assert_eq!(
                bytes,
                *expected,
                "{case} round {round} device {device}: {}",
                describe(expected, &bytes)
            );
        }
    }
}

#[test]
fn single_frame_operations_are_bit_identical() {
    let g = golden();
    assert_case(&g, "nop", Nop::new());
    assert_case(&g, "clear", Clear::new());
    assert_case(&g, "sync", Sync::new());
    assert_case(&g, "force_fan_on", ForceFan::new(true));
    assert_case(&g, "force_fan_off", ForceFan::new(false));
    assert_case(&g, "reads_fpga_state_on", ReadsFpgaState::new(true));
    assert_case(&g, "reads_fpga_state_off", ReadsFpgaState::new(false));
}

#[test]
fn firmware_info_requests_are_bit_identical() {
    let g = golden();
    for (case, ty) in [
        ("info_cpu_major", InfoType::CpuMajor),
        ("info_cpu_minor", InfoType::CpuMinor),
        ("info_fpga_major", InfoType::FpgaMajor),
        ("info_fpga_minor", InfoType::FpgaMinor),
        ("info_fpga_functions", InfoType::FpgaFunctions),
        ("info_clear", InfoType::Clear),
    ] {
        assert_case(&g, case, FirmInfo::new(ty));
    }
}

#[test]
fn silencer_configurations_are_bit_identical() {
    let g = golden();
    let nz = |v| NonZeroU16::new(v).unwrap();
    assert_case(
        &g,
        "silencer_steps",
        Silencer::new(SilencerConfig::FixedCompletionSteps {
            intensity: nz(0x12),
            phase: nz(0x34),
            strict: true,
        }),
    );
    assert_case(
        &g,
        "silencer_steps_loose",
        Silencer::new(SilencerConfig::default_non_strict()),
    );
    assert_case(
        &g,
        "silencer_rate",
        Silencer::new(SilencerConfig::FixedUpdateRate {
            intensity: nz(0x1234),
            phase: nz(0x5678),
        }),
    );
    assert_case(
        &g,
        "silencer_time",
        Silencer::new(SilencerConfig::FixedCompletionTime {
            intensity: Duration::from_micros(250),
            phase: Duration::from_millis(1),
            strict: true,
        }),
    );
}

#[test]
fn gain_is_bit_identical() {
    let g = golden();
    let geo = geometry();
    let emissions = pattern(&geo, 0);
    assert_case(&g, "gain", Gain::new(&emissions));
    assert_case(
        &g,
        "gain_s1_later",
        Gain::with_segment(&emissions, Segment::S1, false),
    );
}

#[test]
fn the_pattern_command_packs_like_the_legacy_gain() {
    use crate::commands::Pattern;
    use autd3_rs_core::value::{PatternBank, TransitionMode as CoreTransitionMode};

    let g = golden();
    let geo = geometry();
    let emissions = pattern(&geo, 0);

    for (case, cmd) in [
        ("gain", Pattern::new(&emissions)),
        (
            "gain_s1_later",
            Pattern {
                transition_mode: CoreTransitionMode::Later,
                ..Pattern::with_bank(PatternBank::B1, &emissions)
            },
        ),
    ] {
        let mut builder = LegacyDatagramBuilder::new(Arc::new(geometry()));
        builder.push(cmd);
        let frames = builder
            .build()
            .unwrap_or_else(|e| panic!("{case}: encode failed: {e}"));
        assert_matches(&g, case, &frames);
    }
}

fn mod_buffer(len: usize) -> Vec<u8> {
    (0..len).map(|i| emission(i).phase.0).collect()
}

#[test]
fn modulation_frame_split_is_bit_identical() {
    let g = golden();
    let short = mod_buffer(100);
    let long = mod_buffer(3000);

    assert_case(
        &g,
        "modulation_short",
        Modulation::new(divide(10), &short, ModulationOption::default()),
    );
    assert_case(
        &g,
        "modulation_long",
        Modulation::new(divide(10), &long, ModulationOption::default()),
    );
    assert_case(
        &g,
        "modulation_s1_finite",
        Modulation::new(
            divide(10),
            &short,
            ModulationOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(5).unwrap()),
                transition_mode: TransitionMode::SyncIdx,
            },
        ),
    );
    assert_case(
        &g,
        "modulation_s1_later",
        Modulation::new(
            divide(10),
            &short,
            ModulationOption {
                segment: Segment::S1,
                transition_mode: TransitionMode::Later,
                ..ModulationOption::default()
            },
        ),
    );
    assert_case(
        &g,
        "modulation_s1_ext",
        Modulation::new(
            divide(10),
            &short,
            ModulationOption {
                segment: Segment::S1,
                transition_mode: TransitionMode::Ext,
                ..ModulationOption::default()
            },
        ),
    );
}

#[test]
fn modulation_boundary_sizes_are_bit_identical() {
    let g = golden();
    for (case, len) in [
        ("modulation_min", 2),
        ("modulation_odd", 101),
        ("modulation_head_full", 254),
        ("modulation_head_plus_one", 255),
        ("modulation_frame_boundary", 254 + 618),
    ] {
        let buffer = mod_buffer(len);
        assert_case(
            &g,
            case,
            Modulation::new(divide(10), &buffer, ModulationOption::default()),
        );
    }
}

#[test]
fn foci_stm_frame_split_is_bit_identical() {
    let g = golden();

    let single = (0..300)
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
    assert_case(
        &g,
        "foci_stm_1",
        FociStm::new(divide(0xFFFF), &single, FociStmOption::default()),
    );
    assert_case(
        &g,
        "foci_stm_s1_later",
        FociStm::new(
            divide(0xFFFF),
            &single,
            FociStmOption {
                segment: Segment::S1,
                transition_mode: TransitionMode::Later,
                ..FociStmOption::default()
            },
        ),
    );

    let dual = (0..120)
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
    assert_case(
        &g,
        "foci_stm_2",
        FociStm::new(divide(0xFFFF), &dual, FociStmOption::default()),
    );

    let octuple = (0..12)
        .map(|i| {
            ControlPoints::new(
                core::array::from_fn(|j| {
                    ControlPoint::new(
                        Point3::new(
                            80.0 + j as f32 * 5.0,
                            60.0 + i as f32,
                            150.0 + (i * 8 + j) as f32 * 0.25,
                        ),
                        Phase(u8::try_from(j * 16).unwrap()),
                    )
                }),
                Intensity(0x70),
            )
        })
        .collect::<Vec<ControlPoints<8>>>();
    assert_case(
        &g,
        "foci_stm_8",
        FociStm::new(divide(0xFFFF), &octuple, FociStmOption::default()),
    );
}

#[test]
fn stm_header_transition_fields_are_bit_identical() {
    let g = golden();

    let single = (0..300)
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
    assert_case(
        &g,
        "foci_stm_s1_finite",
        FociStm::new(
            divide(0xFFFF),
            &single,
            FociStmOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(3).unwrap()),
                transition_mode: TransitionMode::SyncIdx,
                ..FociStmOption::default()
            },
        ),
    );
    assert_case(
        &g,
        "foci_stm_s1_sys_time",
        FociStm::new(
            divide(0xFFFF),
            &single,
            FociStmOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(7).unwrap()),
                transition_mode: TransitionMode::SysTime(DcSysTime::from_nanos(
                    0x0123_4567_89AB_CDEF,
                )),
                ..FociStmOption::default()
            },
        ),
    );

    let geo = geometry();
    let patterns = (0..5).map(|k| pattern(&geo, k * 7)).collect::<Vec<_>>();
    assert_case(
        &g,
        "gain_stm_s1_finite",
        GainStm::new(
            divide(0xFFFF),
            &patterns,
            GainStmOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(2).unwrap()),
                transition_mode: TransitionMode::SyncIdx,
                ..GainStmOption::default()
            },
        ),
    );
    assert_case(
        &g,
        "gain_stm_s1_gpio",
        GainStm::new(
            divide(0xFFFF),
            &patterns,
            GainStmOption {
                segment: Segment::S1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(4).unwrap()),
                transition_mode: TransitionMode::Gpio(GpioIn::I1),
                ..GainStmOption::default()
            },
        ),
    );
}

#[test]
fn gain_stm_frame_split_is_bit_identical() {
    let g = golden();
    let geo = geometry();
    let patterns = (0..5).map(|k| pattern(&geo, k * 7)).collect::<Vec<_>>();

    for (case, mode) in [
        ("gain_stm_full", GainStmMode::PhaseIntensityFull),
        ("gain_stm_phase_full", GainStmMode::PhaseFull),
        ("gain_stm_phase_half", GainStmMode::PhaseHalf),
    ] {
        assert_case(
            &g,
            case,
            GainStm::new(
                divide(0xFFFF),
                &patterns,
                GainStmOption {
                    mode,
                    ..GainStmOption::default()
                },
            ),
        );
    }

    assert_case(
        &g,
        "gain_stm_s1_later",
        GainStm::new(
            divide(0xFFFF),
            &patterns,
            GainStmOption {
                segment: Segment::S1,
                transition_mode: TransitionMode::Later,
                ..GainStmOption::default()
            },
        ),
    );

    let remainder = (0..7).map(|k| pattern(&geo, k * 7)).collect::<Vec<_>>();
    assert_case(
        &g,
        "gain_stm_phase_half_remainder",
        GainStm::new(
            divide(0xFFFF),
            &remainder,
            GainStmOption {
                mode: GainStmMode::PhaseHalf,
                ..GainStmOption::default()
            },
        ),
    );
}

#[test]
fn transducer_wide_tables_are_bit_identical() {
    let g = golden();
    let geo = geometry();

    let masks = geo
        .iter()
        .map(|d| (0..d.num_transducers()).map(|i| i % 3 != 0).collect())
        .collect::<Vec<Vec<bool>>>();
    assert_case(&g, "output_mask", SetOutputMask::new(&masks, Segment::S0));
    assert_case(
        &g,
        "output_mask_s1",
        SetOutputMask::new(&masks, Segment::S1),
    );

    let phases = geo
        .iter()
        .map(|d| {
            (0..d.num_transducers())
                .map(|i| Phase(u8::try_from((i * 3) % 256).unwrap()))
                .collect()
        })
        .collect::<Vec<Vec<Phase>>>();
    assert_case(&g, "phase_correction", SetPhaseCorrection::new(&phases));

    let table: [PulseWidth; PWE_TABLE_SIZE] =
        core::array::from_fn(|i| PulseWidth::new(u16::try_from(i).unwrap() * 2));
    assert_case(&g, "pulse_width_encoder", SetPulseWidthTable::new(&table));
}

#[test]
fn gpio_operations_are_bit_identical() {
    let g = golden();
    assert_case(
        &g,
        "fpga_gpio_out",
        SetGpioOut::new([
            GpioOut::BaseSignal,
            GpioOut::ModIdx(0x1234),
            GpioOut::PwmOut(3),
            GpioOut::SysTimeEq(DcSysTime::from_nanos(0x0000_1234_5678_9AB0)),
        ]),
    );
    assert_case(
        &g,
        "emulate_gpio_in",
        EmulateGpioIn::new([true, false, true, false]),
    );
}

#[test]
fn change_segment_operations_are_bit_identical() {
    let g = golden();
    assert_case(&g, "swap_gain", LegacyChangePatternBank::gain(Segment::S1));
    assert_case(
        &g,
        "swap_modulation",
        LegacyChangePatternBank::modulation(Segment::S1, TransitionMode::Immediate),
    );
    assert_case(
        &g,
        "swap_foci_stm",
        LegacyChangePatternBank::foci_stm(
            Segment::S1,
            TransitionMode::SysTime(DcSysTime::from_nanos(0x0123_4567_89AB_CDEF)),
        ),
    );
    assert_case(
        &g,
        "swap_gain_stm",
        LegacyChangePatternBank::gain_stm(Segment::S0, TransitionMode::Gpio(GpioIn::I2)),
    );
}

#[test]
fn a_two_operation_tuple_is_split_into_two_identical_slot_1_frames() {
    let g = golden();
    let mut builder = LegacyDatagramBuilder::new(Arc::new(geometry()));
    builder.push_op(Clear::new()).push_op(Sync::new());
    let frames = builder.build().unwrap();

    assert_eq!(frames.len(), 2, "no slot-2 fusion");
    let fused = &g["clear_sync"][0];
    for (round, case) in ["clear", "sync"].into_iter().enumerate() {
        let mut bytes = frames.frame(round).unwrap().frames()[0].to_bytes();
        bytes[0] = 0;
        assert_eq!(bytes, g[case][0][0], "{case} frame is unchanged");
    }
    assert_eq!(
        fused[0][2], 2,
        "the legacy SDK put the second operation at payload offset 2"
    );
}

#[test]
fn every_golden_case_is_covered() {
    const CASES: usize = 53;
    assert_eq!(
        golden().len(),
        CASES,
        "a golden case was added without a matching assertion"
    );
}

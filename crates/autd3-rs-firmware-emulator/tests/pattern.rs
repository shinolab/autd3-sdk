#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/pattern.rs"]
mod pattern;

use core::num::NonZeroU16;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::fpga_params::REP_INFINITE;
use autd3_cpu_wire::payload::{
    ConfigPatternPayload, EmissionType, PhaseDepth, TransitionModeKind, WritePatternFusedPayload,
    WritePatternPhasePayload, WritePatternRawPayload,
};
use autd3_cpu_wire::value::TransitionMode;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_firmware_emulator::Device;
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U16;

use common::{NUM_TRANSDUCERS, frame};
use pattern::{activate_pattern_bank, config_pattern};

const BANK: PatternBank = PatternBank::B1;

fn write_phase_header(depth: PhaseDepth, count: u8, intensity: u8) -> Vec<u8> {
    WritePatternPhasePayload {
        bank: BANK,
        depth,
        count,
        intensity,
        index: U16::new(0),
    }
    .as_bytes()
    .to_vec()
}

fn interleave<'a>(phases: &'a [u8], intensities: &'a [u8]) -> impl Iterator<Item = u8> + 'a {
    phases
        .iter()
        .zip(intensities)
        .flat_map(|(&phase, &intensity)| [phase, intensity])
}

#[test]
fn raw_pattern_round_trips_to_emissions() {
    let expected: (Vec<Phase>, Vec<Intensity>) = (
        (0..NUM_TRANSDUCERS).map(|i| Phase(i as u8)).collect(),
        (0..NUM_TRANSDUCERS)
            .map(|i| Intensity((255 - i) as u8))
            .collect(),
    );

    let mut write = WritePatternRawPayload {
        bank: BANK,
        count: 1,
        index: U16::new(0),
    }
    .as_bytes()
    .to_vec();
    write.extend(
        expected
            .0
            .iter()
            .zip(&expected.1)
            .flat_map(|(p, i)| [p.0, i.0]),
    );

    let config = config_pattern(BANK, EmissionType::Raw, 1, 0, 0, REP_INFINITE);
    let change = activate_pattern_bank(BANK, TransitionModeKind::Immediate);

    let mut device = Device::new(NUM_TRANSDUCERS);
    assert_eq!(
        device.send(&frame(Cmd::WritePatternRaw, &write)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device.send(&frame(Cmd::ConfigPattern, &config)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(Cmd::ActivatePatternBank, &change))
            .status,
        DeviceErrorCode::None
    );

    assert_eq!(BANK as u16, device.fpga().req_pattern_bank());
    assert_eq!(
        u16::from(EmissionType::Raw.as_u8()),
        device.fpga().pattern_mode(BANK as usize)
    );
    assert_eq!(expected, device.fpga().emissions_at(BANK as usize, 0));
}

#[test]
fn a_fused_pattern_leaves_the_fpga_as_write_config_activate_does() {
    let phases: Vec<u8> = (0..NUM_TRANSDUCERS).map(|i| (i * 7) as u8).collect();
    let intensities: Vec<u8> = (0..NUM_TRANSDUCERS).map(|i| (255 - i) as u8).collect();

    let mut write = WritePatternRawPayload {
        bank: BANK,
        count: 1,
        index: U16::new(0),
    }
    .as_bytes()
    .to_vec();
    write.extend(interleave(&phases, &intensities));
    let mut split = Device::new(NUM_TRANSDUCERS);
    for (cmd, payload) in [
        (Cmd::WritePatternRaw, write),
        (
            Cmd::ConfigPattern,
            ConfigPatternPayload::raw(BANK, NonZeroU16::MAX, 1, REP_INFINITE)
                .unwrap()
                .as_bytes()
                .to_vec(),
        ),
        (
            Cmd::ActivatePatternBank,
            activate_pattern_bank(BANK, TransitionModeKind::Immediate),
        ),
    ] {
        assert_eq!(
            split.send(&frame(cmd, &payload)).status,
            DeviceErrorCode::None
        );
    }

    let mut fused_payload = WritePatternFusedPayload::new(BANK, TransitionMode::Immediate)
        .as_bytes()
        .to_vec();
    fused_payload.extend(interleave(&phases, &intensities));
    let mut fused = Device::new(NUM_TRANSDUCERS);
    assert_eq!(
        fused
            .send(&frame(Cmd::WritePatternFused, &fused_payload))
            .status,
        DeviceErrorCode::None
    );

    for addr in 0..=u8::MAX {
        assert_eq!(
            split.fpga().controller_reg(u16::from(addr)),
            fused.fpga().controller_reg(u16::from(addr)),
            "controller register {addr:#04x}"
        );
    }
    assert_eq!(BANK as u16, fused.fpga().req_pattern_bank());
    assert_eq!(
        split.fpga().emissions_at(BANK as usize, 0),
        fused.fpga().emissions_at(BANK as usize, 0)
    );
}

fn send_phase_pattern(write: &[u8]) -> Device {
    let config = config_pattern(BANK, EmissionType::Raw, 4, 0, 0, 0);
    let change = activate_pattern_bank(BANK, TransitionModeKind::SyncIdx);
    let mut device = Device::new(NUM_TRANSDUCERS);
    assert_eq!(
        device.send(&frame(Cmd::WritePatternPhase, write)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device.send(&frame(Cmd::ConfigPattern, &config)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(Cmd::ActivatePatternBank, &change))
            .status,
        DeviceErrorCode::None
    );
    device
}

#[test]
fn phase_bits8_pattern_expands_to_consecutive_indices() {
    let phase = |g: usize, t: usize| (t * 3 + g * 17) as u8;

    let mut write = write_phase_header(PhaseDepth::Bits8, 4, 0x80);
    for g in 0..4 {
        write.extend((0..NUM_TRANSDUCERS).map(|t| phase(g, t)));
    }

    let device = send_phase_pattern(&write);
    for g in 0..4 {
        let (phases, intensities) = device.fpga().emissions_at(BANK as usize, g);
        for t in 0..NUM_TRANSDUCERS {
            assert_eq!(phases[t], Phase(phase(g, t)), "g={g} t={t}");
            assert_eq!(intensities[t], Intensity(0x80), "g={g} t={t}");
        }
    }
}

#[test]
fn phase_bits4_pattern_expands_nibbles_to_full_range() {
    let nibble = |g: usize, t: usize| ((t + g) & 0x0F) as u8;

    let mut write = write_phase_header(PhaseDepth::Bits4, 4, 0xFF);
    for g in 0..4 {
        let phases: Vec<u8> = (0..NUM_TRANSDUCERS).map(|t| nibble(g, t) * 0x11).collect();
        let mut packed = vec![0u8; PhaseDepth::Bits4.bytes_per_pattern()];
        PhaseDepth::Bits4.pack(&phases, &mut packed);
        write.extend(packed);
    }

    let device = send_phase_pattern(&write);
    for g in 0..4 {
        let (phases, intensities) = device.fpga().emissions_at(BANK as usize, g);
        for t in 0..NUM_TRANSDUCERS {
            assert_eq!(phases[t], Phase(nibble(g, t) * 0x11), "g={g} t={t}");
            assert_eq!(intensities[t], Intensity(0xFF), "g={g} t={t}");
        }
    }
}

#[test]
fn unknown_command_reports_error() {
    let mut device = Device::new(NUM_TRANSDUCERS);

    let mut bad = frame(Cmd::Nop, &[]);
    bad[1] = 0x7F;
    let rx = device.send(&bad);

    assert_eq!(rx.status, DeviceErrorCode::UnknownCmd);
}

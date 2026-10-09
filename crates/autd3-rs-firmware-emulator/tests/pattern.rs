#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/pattern.rs"]
mod pattern;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{
    EmissionType, PhaseDepth, TransitionMode, WritePatternPhasePayload, WritePatternRawPayload,
};
use autd3_rs_core::params::REP_INFINITE;
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
    write.extend(expected.0.iter().map(|p| p.0));
    write.extend(expected.1.iter().map(|i| i.0));

    let config = config_pattern(BANK, EmissionType::Raw, 1, 0, 0, REP_INFINITE);
    let change = activate_pattern_bank(BANK, TransitionMode::Immediate);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternRaw, &write)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device.send(&frame(1, Cmd::ConfigPattern, &config)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(2, Cmd::ActivatePatternBank, &change))
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

fn send_phase_pattern(write: &[u8]) -> Device {
    let config = config_pattern(BANK, EmissionType::Raw, 4, 0, 0, 0);
    let change = activate_pattern_bank(BANK, TransitionMode::SyncIdx);
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternPhase, write)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device.send(&frame(1, Cmd::ConfigPattern, &config)).status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(2, Cmd::ActivatePatternBank, &change))
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
        write.extend((0..NUM_TRANSDUCERS.div_ceil(2)).map(|i| {
            let hi = if 2 * i + 1 < NUM_TRANSDUCERS {
                nibble(g, 2 * i + 1)
            } else {
                0
            };
            (hi << 4) | nibble(g, 2 * i)
        }));
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
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut bad = frame(0, Cmd::Nop, &[]);
    bad[1] = 0x7F;
    let rx = device.send(&bad);

    assert_eq!(rx.status, DeviceErrorCode::UnknownCmd);
}

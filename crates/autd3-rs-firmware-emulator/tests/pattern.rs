#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/pattern.rs"]
mod pattern;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{
    EmissionType, PhaseDepth, TransitionModeKind, WritePatternPhasePayload,
};
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_firmware_emulator::Device;
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

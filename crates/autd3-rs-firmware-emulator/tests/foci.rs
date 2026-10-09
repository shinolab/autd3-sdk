#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/pattern.rs"]
mod pattern;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{EmissionType, TransitionMode, WriteFociPayload};
use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_core::value::Intensity;
use autd3_rs_firmware_emulator::Device;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U32;

use common::{NUM_TRANSDUCERS, frame};
use pattern::{activate_pattern_bank, config_pattern};

const BANK: PatternBank = PatternBank::B0;
const NUM_FOCI: u8 = 1;
const SOUND_SPEED: u16 = 340;
const FOCUS_INTENSITY: u8 = 0xAA;

#[test]
fn single_focus_synthesizes_phases() {
    let z: u64 = 8192;
    let focus: u64 = (u64::from(FOCUS_INTENSITY) << 54) | (z << 36);

    let mut write = WriteFociPayload {
        bank: BANK,
        reserved: 0,
        offset: U32::new(0),
    }
    .as_bytes()
    .to_vec();
    write.extend_from_slice(&focus.to_le_bytes());
    write.extend_from_slice(&focus.to_le_bytes());

    let config = config_pattern(
        BANK,
        EmissionType::Foci,
        2,
        NUM_FOCI,
        SOUND_SPEED,
        REP_INFINITE,
    );
    let change = activate_pattern_bank(BANK, TransitionMode::Immediate);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WriteFociBuffer, &write)).status,
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
    device.fpga_mut().update_with_sys_time(0);

    assert_eq!(
        u16::from(EmissionType::Foci.as_u8()),
        device.fpga().pattern_mode(BANK as usize)
    );
    assert_eq!(usize::from(NUM_FOCI), device.fpga().num_foci(BANK as usize));

    let (phases, intensities) = device.fpga().emissions();
    assert_eq!(NUM_TRANSDUCERS, phases.len());
    assert_eq!(NUM_TRANSDUCERS, intensities.len());

    assert!(intensities.iter().all(|&i| i == Intensity(FOCUS_INTENSITY)));

    assert!(phases.iter().any(|&p| p != phases[0]));
}

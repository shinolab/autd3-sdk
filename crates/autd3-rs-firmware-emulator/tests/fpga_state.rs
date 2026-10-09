#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/modulation.rs"]
mod modulation;
#[path = "common/pattern.rs"]
mod pattern;

use autd3_cpu_wire::fpga_params::FpgaStateFlags;
use autd3_cpu_wire::payload::{
    EmissionType, TransitionMode, WriteModPayload, WritePatternRawPayload,
};
use autd3_cpu_wire::{ModulationBank, PatternBank};
use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::Cmd;
use autd3_rs_firmware_emulator::Device;
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use zerocopy::IntoBytes;
use zerocopy::little_endian::{U16, U32};

use common::{NUM_TRANSDUCERS, frame};
use modulation::{activate_modulation_bank, config_modulation};
use pattern::{activate_pattern_bank, config_pattern};

const BIT_THERMAL: u8 = FpgaStateFlags::THERMAL_ASSERT.bits();
const BIT_MOD_BANK: u8 = FpgaStateFlags::MOD_BANK.bits();
const BIT_PATTERN_BANK: u8 = FpgaStateFlags::PATTERN_BANK.bits();
const BIT_PATTERN_MODE: u8 = FpgaStateFlags::PATTERN_MODE.bits();

fn write_modulation(bank: ModulationBank, samples: &[u8]) -> Vec<u8> {
    let header = WriteModPayload {
        bank,
        reserved: 0,
        offset: U32::new(0),
    };
    [header.as_bytes(), samples].concat()
}

fn write_pattern(bank: PatternBank) -> Vec<u8> {
    let header = WritePatternRawPayload {
        bank,
        count: 1,
        index: U16::new(0),
    };
    [header.as_bytes(), &[0u8; NUM_TRANSDUCERS * 2]].concat()
}

fn config_raw_pattern(bank: PatternBank, size: u32, rep: u16) -> Vec<u8> {
    config_pattern(bank, EmissionType::Raw, size, 0, 0, rep)
}

fn read_state(device: &mut Device, seq: u8) -> u8 {
    device.send(&frame(seq, Cmd::ReadFpgaState, &[])).data()[0]
}

#[test]
fn default_state_is_pattern_mode_bank_zero() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    assert_eq!(BIT_PATTERN_MODE, read_state(&mut device, 0));
}

#[test]
fn thermal_bit_follows_setter() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    device.fpga_mut().set_thermal(true);
    assert_eq!(BIT_PATTERN_MODE | BIT_THERMAL, read_state(&mut device, 0));

    device.fpga_mut().set_thermal(false);
    assert_eq!(BIT_PATTERN_MODE, read_state(&mut device, 1));
}

#[test]
fn modulation_bank_switch_reflects_in_state() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let samples = [10u8, 20, 30, 40];
    device.send(&frame(
        0,
        Cmd::WriteModulationBuffer,
        &write_modulation(ModulationBank::B1, &samples),
    ));
    device.send(&frame(
        1,
        Cmd::ConfigModulation,
        &config_modulation(ModulationBank::B1, 1, samples.len() as u32, REP_INFINITE),
    ));
    assert_eq!(0, read_state(&mut device, 2) & BIT_MOD_BANK);

    device.send(&frame(
        3,
        Cmd::ActivateModulationBank,
        &activate_modulation_bank(ModulationBank::B1, TransitionMode::Immediate, 0),
    ));
    assert_eq!(1, device.fpga().current_mod_bank());
    assert_eq!(BIT_MOD_BANK, read_state(&mut device, 4) & BIT_MOD_BANK);
}

#[test]
fn pattern_bank_switch_reflects_in_state() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    device.send(&frame(
        0,
        Cmd::WritePatternRaw,
        &write_pattern(PatternBank::B1),
    ));
    device.send(&frame(
        1,
        Cmd::ConfigPattern,
        &config_raw_pattern(PatternBank::B1, 1, REP_INFINITE),
    ));
    assert_eq!(0, read_state(&mut device, 2) & BIT_PATTERN_BANK);

    device.send(&frame(
        3,
        Cmd::ActivatePatternBank,
        &activate_pattern_bank(PatternBank::B1, TransitionMode::Immediate),
    ));
    assert_eq!(1, device.fpga().current_pattern_bank());
    let state = read_state(&mut device, 4);
    assert_eq!(BIT_PATTERN_BANK | BIT_PATTERN_MODE, state);
}

#[test]
fn multi_index_pattern_reports_stm_mode() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    device.send(&frame(
        0,
        Cmd::ConfigPattern,
        &config_raw_pattern(PatternBank::B0, 4, REP_INFINITE),
    ));
    device.send(&frame(
        1,
        Cmd::ActivatePatternBank,
        &activate_pattern_bank(PatternBank::B0, TransitionMode::Immediate),
    ));

    assert!(!device.fpga().is_pattern_mode());
    assert_eq!(0, read_state(&mut device, 2) & BIT_PATTERN_MODE);
}

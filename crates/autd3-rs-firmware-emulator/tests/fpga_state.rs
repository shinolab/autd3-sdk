#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const REP_INFINITE: u16 = 0xFFFF;
const IMMEDIATE: u8 = 0xFF;

const BIT_THERMAL: u8 = 1 << 0;
const BIT_MOD_BANK: u8 = 1 << 1;
const BIT_PATTERN_BANK: u8 = 1 << 2;
const BIT_PATTERN_MODE: u8 = 1 << 3;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut tx = TxFrame::new(Seq::new(seq), cmd);
    tx.payload[..payload.len()].copy_from_slice(payload);
    let mut buf = [0u8; TX_FRAME_BYTES];
    tx.write_to(&mut buf);
    buf
}

fn write_modulation(bank: u8, samples: &[u8]) -> Vec<u8> {
    let mut w = vec![bank, 0];
    w.extend_from_slice(&0u32.to_le_bytes());
    w.extend_from_slice(&(samples.len() as u16).to_le_bytes());
    w.extend_from_slice(samples);
    w
}

fn config_modulation(bank: u8, size: usize, rep: u16) -> Vec<u8> {
    let mut c = vec![bank, 0];
    c.extend_from_slice(&1u16.to_le_bytes());
    c.extend_from_slice(&(size as u32).to_le_bytes());
    c.extend_from_slice(&rep.to_le_bytes());
    c
}

fn change_mod_bank(bank: u8) -> Vec<u8> {
    let mut c = vec![bank, IMMEDIATE];
    c.extend_from_slice(&0u64.to_le_bytes());
    c
}

fn write_pattern(bank: u8, indices: usize) -> Vec<u8> {
    let mut w = vec![bank, 0];
    w.extend_from_slice(&0u32.to_le_bytes());
    w.extend_from_slice(&((NUM_TRANSDUCERS * 2 * indices) as u16).to_le_bytes());
    w.extend(std::iter::repeat_n(0u8, NUM_TRANSDUCERS * 2 * indices));
    w
}

fn config_pattern(bank: u8, size: usize, rep: u16) -> Vec<u8> {
    let mut c = vec![0u8; 14];
    c[0] = bank;
    c[1] = 1;
    c[2..4].copy_from_slice(&512u16.to_le_bytes());
    c[4..8].copy_from_slice(&(size as u32).to_le_bytes());
    c[12..14].copy_from_slice(&rep.to_le_bytes());
    c
}

fn change_pattern_bank(bank: u8) -> Vec<u8> {
    let mut c = vec![bank, IMMEDIATE];
    c.extend_from_slice(&0u64.to_le_bytes());
    c
}

fn read_state(device: &mut Device, seq: u8) -> u8 {
    device.send(&frame(seq, Cmd::ReadFpgaState, &[])).data
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
        &write_modulation(1, &samples),
    ));
    device.send(&frame(
        1,
        Cmd::ConfigModulation,
        &config_modulation(1, samples.len(), REP_INFINITE),
    ));
    assert_eq!(0, read_state(&mut device, 2) & BIT_MOD_BANK);

    device.send(&frame(3, Cmd::ChangeModulationBank, &change_mod_bank(1)));
    assert_eq!(1, device.fpga().current_mod_bank());
    assert_eq!(BIT_MOD_BANK, read_state(&mut device, 4) & BIT_MOD_BANK);
}

#[test]
fn pattern_bank_switch_reflects_in_state() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    device.send(&frame(0, Cmd::WritePatternBuffer, &write_pattern(1, 1)));
    device.send(&frame(
        1,
        Cmd::ConfigPattern,
        &config_pattern(1, 1, REP_INFINITE),
    ));
    assert_eq!(0, read_state(&mut device, 2) & BIT_PATTERN_BANK);

    device.send(&frame(3, Cmd::ChangePatternBank, &change_pattern_bank(1)));
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
        &config_pattern(0, 4, REP_INFINITE),
    ));
    device.send(&frame(1, Cmd::ChangePatternBank, &change_pattern_bank(0)));

    assert!(!device.fpga().is_pattern_mode());
    assert_eq!(0, read_state(&mut device, 2) & BIT_PATTERN_MODE);
}

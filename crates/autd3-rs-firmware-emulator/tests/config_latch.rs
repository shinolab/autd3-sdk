#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TxFrame};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 1;
const DIVIDER: u16 = 512;
const SIZE: u32 = 64;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    TxFrame::with_payload(Seq::new(seq), cmd, payload).to_vec()
}

fn config_modulation() -> Vec<u8> {
    let mut config = vec![0u8; 10];
    config[0] = BANK;
    config[2..4].copy_from_slice(&DIVIDER.to_le_bytes());
    config[4..8].copy_from_slice(&SIZE.to_le_bytes());
    config[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
    config
}

fn activate_modulation_bank() -> Vec<u8> {
    let mut change = vec![0u8; 14];
    change[0] = BANK;
    change[1] = 0xFF;
    change
}

#[test]
fn config_takes_effect_only_at_the_bank_activation_latch() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    let cycle_at_boot = device.fpga().modulation_cycle(BANK as usize);
    let div_at_boot = device.fpga().modulation_freq_div(BANK as usize);

    assert_eq!(
        device
            .send(&frame(0, Cmd::ConfigModulation, &config_modulation()))
            .status,
        0
    );
    assert_eq!(
        device.fpga().modulation_cycle(BANK as usize),
        cycle_at_boot,
        "config alone must not reach the playback settings"
    );
    assert_eq!(
        device.fpga().modulation_freq_div(BANK as usize),
        div_at_boot
    );

    assert_eq!(
        device
            .send(&frame(
                1,
                Cmd::ActivateModulationBank,
                &activate_modulation_bank()
            ))
            .status,
        0
    );
    assert_eq!(device.fpga().modulation_cycle(BANK as usize), SIZE as usize);
    assert_eq!(device.fpga().modulation_freq_div(BANK as usize), DIVIDER);
}

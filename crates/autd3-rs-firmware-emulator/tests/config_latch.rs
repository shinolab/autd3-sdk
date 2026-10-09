#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/modulation.rs"]
mod modulation;

use autd3_cpu_wire::ModulationBank;
use autd3_cpu_wire::payload::TransitionMode;
use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_firmware_emulator::Device;

use common::{NUM_TRANSDUCERS, frame};
use modulation::{activate_modulation_bank, config_modulation};

const BANK: ModulationBank = ModulationBank::B1;
const DIVIDER: u16 = 512;
const SIZE: u32 = 64;

#[test]
fn config_takes_effect_only_at_the_bank_activation_latch() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    let cycle_at_boot = device.fpga().modulation_cycle(BANK as usize);
    let div_at_boot = device.fpga().modulation_freq_div(BANK as usize);

    assert_eq!(
        device
            .send(&frame(
                0,
                Cmd::ConfigModulation,
                &config_modulation(BANK, DIVIDER, SIZE, REP_INFINITE)
            ))
            .status,
        DeviceErrorCode::None
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
                &activate_modulation_bank(BANK, TransitionMode::Immediate, 0)
            ))
            .status,
        DeviceErrorCode::None
    );
    assert_eq!(device.fpga().modulation_cycle(BANK as usize), SIZE as usize);
    assert_eq!(device.fpga().modulation_freq_div(BANK as usize), DIVIDER);
}

#![allow(clippy::cast_possible_truncation)]

mod common;
#[path = "common/modulation.rs"]
mod modulation;

use autd3_cpu_wire::ModulationBank;
use autd3_cpu_wire::cpu_params::SYS_TIME_TRANSITION_MARGIN;
use autd3_cpu_wire::payload::{GpioInPayload, TransitionMode, WriteModPayload};
use autd3_rs_core::params::{REP_INFINITE, ULTRASOUND_FREQ_HZ};
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_firmware_emulator::Device;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U32;

use common::{NUM_TRANSDUCERS, frame};
use modulation::{activate_modulation_bank, config_modulation};

const ULTRASOUND_PERIOD_NS: u64 = 1_000_000_000 / ULTRASOUND_FREQ_HZ as u64;
const SAMPLES: [u8; 4] = [10, 20, 30, 40];

fn write_modulation(bank: ModulationBank, samples: &[u8]) -> Vec<u8> {
    let header = WriteModPayload {
        bank,
        reserved: 0,
        offset: U32::new(0),
    };
    [header.as_bytes(), samples].concat()
}

#[test]
fn modulation_buffer_and_index_follow_time() {
    let samples = SAMPLES;
    let bank = ModulationBank::B0;
    let divider = 1u16;

    let write = write_modulation(bank, &samples);

    let config = config_modulation(bank, divider, samples.len() as u32, REP_INFINITE);
    let change = activate_modulation_bank(bank, TransitionMode::Immediate, 0);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device
            .send(&frame(0, Cmd::WriteModulationBuffer, &write))
            .status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(1, Cmd::ConfigModulation, &config))
            .status,
        DeviceErrorCode::None
    );
    assert_eq!(
        device
            .send(&frame(2, Cmd::ActivateModulationBank, &change))
            .status,
        DeviceErrorCode::None
    );

    assert_eq!(samples.len(), device.fpga().modulation_cycle(bank as usize));
    assert_eq!(
        samples.to_vec(),
        device.fpga().modulation_buffer(bank as usize)
    );

    for (i, &expected) in [10u8, 20, 30, 40, 10, 20].iter().enumerate() {
        device
            .fpga_mut()
            .update_with_sys_time(i as u64 * ULTRASOUND_PERIOD_NS);
        assert_eq!(i % 4, device.fpga().current_mod_idx());
        assert_eq!(expected, device.fpga().modulation());
    }
}

#[test]
fn modulation_finite_loop_stops_after_rep() {
    let samples = SAMPLES;
    let bank = ModulationBank::B1;
    let divider = 1u16;
    let rep = 1u16;

    let write = write_modulation(bank, &samples);

    let config = config_modulation(bank, divider, samples.len() as u32, rep);
    let change = activate_modulation_bank(bank, TransitionMode::SyncIdx, 0);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.send(&frame(0, Cmd::WriteModulationBuffer, &write));
    device.send(&frame(1, Cmd::ConfigModulation, &config));
    device.send(&frame(2, Cmd::ActivateModulationBank, &change));

    let mut indices = Vec::new();
    for i in 0..24u64 {
        device
            .fpga_mut()
            .update_with_sys_time(i * ULTRASOUND_PERIOD_NS);
        indices.push(device.fpga().current_mod_idx());
    }

    assert_eq!(*indices.last().unwrap(), samples.len() - 1, "{indices:?}");
    assert!(
        indices.windows(2).rev().take(4).all(|w| w[0] == w[1]),
        "playback must be stopped (index frozen): {indices:?}"
    );
}

#[test]
fn sys_time_transition_within_margin_is_rejected() {
    const MARGIN_NS: u64 = SYS_TIME_TRANSITION_MARGIN.as_nanos() as u64;

    let samples = SAMPLES;
    let bank = ModulationBank::B1;

    let write = write_modulation(bank, &samples);

    let config = config_modulation(bank, 1, samples.len() as u32, 3);

    let sys_time = 1_000_000_000u64;
    let change = |value: u64| activate_modulation_bank(bank, TransitionMode::SysTime, value);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.send(&frame(0, Cmd::WriteModulationBuffer, &write));
    device.send(&frame(1, Cmd::ConfigModulation, &config));
    device.fpga_mut().update_with_sys_time(sys_time);

    assert_eq!(
        device
            .send(&frame(
                2,
                Cmd::ActivateModulationBank,
                &change(sys_time + MARGIN_NS - 1)
            ))
            .status,
        DeviceErrorCode::MissTransitionTime,
        "transition within the margin must be rejected"
    );

    assert_eq!(
        device
            .send(&frame(
                3,
                Cmd::ActivateModulationBank,
                &change(sys_time + MARGIN_NS)
            ))
            .status,
        DeviceErrorCode::None,
        "transition at least a margin ahead is accepted"
    );
}

#[test]
fn sys_time_transition_margin_follows_the_cpu_config() {
    use autd3_cpu_wire::config::CpuConfig;
    use autd3_cpu_wire::payload::SetCpuConfigPayload;

    const MARGIN_NS: u32 = 1_000_000;

    let samples = SAMPLES;
    let bank = ModulationBank::B1;

    let write = write_modulation(bank, &samples);

    let config = config_modulation(bank, 1, samples.len() as u32, 3);

    let sys_time = 1_000_000_000u64;
    let change = |value: u64| activate_modulation_bank(bank, TransitionMode::SysTime, value);
    let cpu_config = SetCpuConfigPayload::encode(&CpuConfig {
        sys_time_transition_margin: std::time::Duration::from_nanos(u64::from(MARGIN_NS)),
        ..CpuConfig::default()
    })
    .unwrap();

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.send(&frame(0, Cmd::WriteModulationBuffer, &write));
    device.send(&frame(1, Cmd::ConfigModulation, &config));
    assert_eq!(
        device
            .send(&frame(2, Cmd::SetCpuConfig, cpu_config.as_bytes()))
            .status,
        DeviceErrorCode::None
    );
    device.fpga_mut().update_with_sys_time(sys_time);

    assert_eq!(
        device
            .send(&frame(
                3,
                Cmd::ActivateModulationBank,
                &change(sys_time + u64::from(MARGIN_NS) - 1)
            ))
            .status,
        DeviceErrorCode::MissTransitionTime,
    );
    assert_eq!(
        device
            .send(&frame(
                4,
                Cmd::ActivateModulationBank,
                &change(sys_time + u64::from(MARGIN_NS))
            ))
            .status,
        DeviceErrorCode::None,
    );
}

#[test]
fn gpio_transition_waits_for_emulated_gpio_in() {
    const GPIO_IN_PIN: u64 = 0;

    let samples = SAMPLES;
    let bank = ModulationBank::B1;

    let write = write_modulation(bank, &samples);

    let config = config_modulation(bank, 1, samples.len() as u32, 1);
    let change = activate_modulation_bank(bank, TransitionMode::Gpio, GPIO_IN_PIN);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.send(&frame(0, Cmd::WriteModulationBuffer, &write));
    device.send(&frame(1, Cmd::ConfigModulation, &config));
    assert_eq!(
        device
            .send(&frame(2, Cmd::ActivateModulationBank, &change))
            .status,
        DeviceErrorCode::None
    );

    for i in 1..8u64 {
        device
            .fpga_mut()
            .update_with_sys_time(i * ULTRASOUND_PERIOD_NS);
    }
    assert_eq!(
        0,
        device.fpga().current_mod_bank(),
        "GPIO-in is low: the bank must not switch"
    );

    let gpio_in = GpioInPayload {
        gpio_in_0: true,
        gpio_in_1: false,
        gpio_in_2: false,
        gpio_in_3: false,
    };
    assert_eq!(
        device
            .send(&frame(3, Cmd::EmulateGpioIn, gpio_in.as_bytes()))
            .status,
        DeviceErrorCode::None
    );
    for i in 8..16u64 {
        device
            .fpga_mut()
            .update_with_sys_time(i * ULTRASOUND_PERIOD_NS);
    }
    assert_eq!(
        bank as usize,
        device.fpga().current_mod_bank(),
        "EmulateGpioIn asserted the pin: the bank must switch"
    );
}

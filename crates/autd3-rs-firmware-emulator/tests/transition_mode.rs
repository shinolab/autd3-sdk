mod common;
#[path = "common/modulation.rs"]
mod modulation;

use autd3_cpu_wire::ModulationBank;
use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::fpga_params::{FpgaStateFlags, REP_INFINITE};
use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::{
    ActivatePatternBankPayload, EmissionType, SILENCER_DEFAULT_UPDATE_RATE, SilencerFlags,
    TransitionModeKind, WritePatternFusedPayload,
};
use autd3_cpu_wire::value::TransitionMode;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_firmware_emulator::Device;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U64;

use common::{NUM_TRANSDUCERS, frame};

fn send(device: &mut Device, cmd: Cmd, payload: &[u8]) -> DeviceErrorCode {
    device.send(&frame(cmd, payload)).status
}

fn config_pattern(device: &mut Device, bank: PatternBank, divider: u16, rep: u16) {
    let payload: Vec<u8> = [bank.as_u8(), EmissionType::Raw.as_u8()]
        .into_iter()
        .chain(divider.to_le_bytes())
        .chain(2u32.to_le_bytes())
        .chain([0])
        .chain(0u16.to_le_bytes())
        .chain(rep.to_le_bytes())
        .collect();
    assert_eq!(
        send(device, Cmd::ConfigPattern, &payload),
        DeviceErrorCode::None
    );
}

fn activate_pattern_bank(
    device: &mut Device,
    bank: PatternBank,
    mode: TransitionModeKind,
) -> DeviceErrorCode {
    let payload = ActivatePatternBankPayload {
        bank,
        transition_mode: mode,
        transition_value: U64::new(0),
    };
    send(device, Cmd::ActivatePatternBank, payload.as_bytes())
}

fn config_modulation(device: &mut Device, bank: ModulationBank, divider: u16, rep: u16) {
    assert_eq!(
        send(
            device,
            Cmd::ConfigModulation,
            &modulation::config_modulation(bank, divider, 2, rep),
        ),
        DeviceErrorCode::None
    );
}

fn activate_modulation_bank(
    device: &mut Device,
    bank: ModulationBank,
    mode: TransitionModeKind,
) -> DeviceErrorCode {
    send(
        device,
        Cmd::ActivateModulationBank,
        &modulation::activate_modulation_bank(bank, mode, 0),
    )
}

fn pending(device: &Device) -> FpgaStateFlags {
    FpgaStateFlags::from_bits_retain(device.fpga().fpga_state())
        & (FpgaStateFlags::MOD_TRANSITION_PENDING | FpgaStateFlags::PATTERN_TRANSITION_PENDING)
}

const CASES: [(u16, TransitionModeKind, bool); 10] = [
    (0, TransitionModeKind::Immediate, false),
    (0, TransitionModeKind::Ext, false),
    (0, TransitionModeKind::SyncIdx, true),
    (0, TransitionModeKind::SysTime, true),
    (0, TransitionModeKind::Gpio, true),
    (REP_INFINITE, TransitionModeKind::SyncIdx, false),
    (REP_INFINITE, TransitionModeKind::SysTime, false),
    (REP_INFINITE, TransitionModeKind::Gpio, false),
    (REP_INFINITE, TransitionModeKind::Immediate, true),
    (REP_INFINITE, TransitionModeKind::Ext, true),
];

#[test]
fn modulation_accepts_only_the_modes_of_its_loop() {
    for (rep, mode, valid) in CASES {
        let mut device = Device::new(NUM_TRANSDUCERS);
        config_modulation(&mut device, ModulationBank::B1, 10, rep);

        let status = activate_modulation_bank(&mut device, ModulationBank::B1, mode);

        if valid {
            assert_eq!(status, DeviceErrorCode::None, "{rep:#x} {mode:?}");
        } else {
            assert_eq!(
                status,
                DeviceErrorCode::InvalidTransitionMode,
                "{rep:#x} {mode:?}"
            );
            assert_eq!(device.fpga().current_mod_bank(), 0);
            assert!(pending(&device).is_empty());
        }
    }
}

#[test]
fn pattern_accepts_only_the_modes_of_its_loop() {
    for (rep, mode, valid) in CASES {
        let mut device = Device::new(NUM_TRANSDUCERS);
        config_pattern(&mut device, PatternBank::B1, 10, rep);

        let status = activate_pattern_bank(&mut device, PatternBank::B1, mode);

        if valid {
            assert_eq!(status, DeviceErrorCode::None, "{rep:#x} {mode:?}");
        } else {
            assert_eq!(
                status,
                DeviceErrorCode::InvalidTransitionMode,
                "{rep:#x} {mode:?}"
            );
            assert_eq!(device.fpga().current_pattern_bank(), 0);
            assert!(pending(&device).is_empty());
        }
    }
}

#[test]
fn a_strict_violation_is_reported_apart_from_a_transition_violation() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    let silencer: Vec<u8> = [SilencerFlags::STRICT_MODE.bits()]
        .into_iter()
        .chain(SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes())
        .chain(SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes())
        .chain(10u16.to_le_bytes())
        .chain(10u16.to_le_bytes())
        .collect();
    config_modulation(&mut device, ModulationBank::B1, 100, REP_INFINITE);
    assert_eq!(
        activate_modulation_bank(
            &mut device,
            ModulationBank::B1,
            TransitionModeKind::Immediate
        ),
        DeviceErrorCode::None
    );
    assert_eq!(
        send(&mut device, Cmd::SetSilencer, &silencer),
        DeviceErrorCode::None
    );

    config_modulation(&mut device, ModulationBank::B0, 5, REP_INFINITE);
    assert_eq!(
        activate_modulation_bank(
            &mut device,
            ModulationBank::B0,
            TransitionModeKind::Immediate
        ),
        DeviceErrorCode::InvalidSilencerSetting
    );

    config_modulation(&mut device, ModulationBank::B0, 100, 0);
    assert_eq!(
        activate_modulation_bank(
            &mut device,
            ModulationBank::B0,
            TransitionModeKind::Immediate
        ),
        DeviceErrorCode::InvalidTransitionMode
    );
    assert_eq!(device.fpga().current_mod_bank(), 1);

    assert_eq!(
        activate_modulation_bank(&mut device, ModulationBank::B1, TransitionModeKind::SyncIdx),
        DeviceErrorCode::InvalidTransitionMode
    );
    config_modulation(&mut device, ModulationBank::B0, 5, REP_INFINITE);
    assert_eq!(
        activate_modulation_bank(
            &mut device,
            ModulationBank::B0,
            TransitionModeKind::Immediate
        ),
        DeviceErrorCode::InvalidSilencerSetting
    );
}

#[test]
fn write_pattern_fused_accepts_only_the_modes_of_an_infinite_loop() {
    for (mode, valid) in [
        (TransitionMode::Immediate, true),
        (TransitionMode::Ext, true),
        (TransitionMode::SyncIdx, false),
    ] {
        let mut device = Device::new(NUM_TRANSDUCERS);
        let payload: Vec<u8> = WritePatternFusedPayload::new(PatternBank::B1, mode)
            .as_bytes()
            .iter()
            .copied()
            .chain([0; PATTERN_RAW_DATA_LEN])
            .collect();

        let status = send(&mut device, Cmd::WritePatternFused, &payload);

        if valid {
            assert_eq!(status, DeviceErrorCode::None, "{mode:?}");
            assert_eq!(device.fpga().current_pattern_bank(), 1);
        } else {
            assert_eq!(status, DeviceErrorCode::InvalidTransitionMode, "{mode:?}");
            assert_eq!(device.fpga().current_pattern_bank(), 0);
        }
    }
}

mod common;
#[path = "common/modulation.rs"]
mod modulation;

use core::num::NonZeroU8;

use autd3_cpu_wire::ModulationBank::{self, B0, B1};
use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{
    ActivatePatternBankPayload, ConfigPatternPayload, EmissionType,
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE, SilencerFlags, SilencerPayload, TransitionMode,
};
use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_firmware_emulator::Device;
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use zerocopy::IntoBytes;
use zerocopy::little_endian::{U16, U32, U64};

use common::{NUM_TRANSDUCERS, frame};

const STRICT: SilencerFlags = SilencerFlags::STRICT_MODE;
const FIXED_UPDATE_RATE: SilencerFlags = SilencerFlags::FIXED_UPDATE_RATE_MODE;
const IMMEDIATE: TransitionMode = TransitionMode::Immediate;
const SYNC_IDX: TransitionMode = TransitionMode::SyncIdx;
const SYS_TIME: TransitionMode = TransitionMode::SysTime;
const EXT: TransitionMode = TransitionMode::Ext;

struct Host {
    device: Device,
    seq: u8,
}

impl Host {
    fn new() -> Self {
        let mut device = Device::new(NUM_TRANSDUCERS);
        device.send(&frame(0, Cmd::Reset, &[]));
        Self { device, seq: 0 }
    }

    fn send(&mut self, cmd: Cmd, payload: &[u8]) -> DeviceErrorCode {
        let frame = frame(self.seq, cmd, payload);
        self.seq = self.seq.wrapping_add(1);
        self.device.send(&frame).status
    }

    fn config_modulation(
        &mut self,
        bank: ModulationBank,
        divider: u16,
        rep: u16,
    ) -> DeviceErrorCode {
        self.send(
            Cmd::ConfigModulation,
            &modulation::config_modulation(bank, divider, 2, rep),
        )
    }

    fn activate_modulation_bank(
        &mut self,
        bank: ModulationBank,
        mode: TransitionMode,
        value: u64,
    ) -> DeviceErrorCode {
        self.send(
            Cmd::ActivateModulationBank,
            &modulation::activate_modulation_bank(bank, mode, value),
        )
    }

    fn config_pattern(&mut self, bank: PatternBank, divider: u16) -> DeviceErrorCode {
        let payload = ConfigPatternPayload {
            bank,
            emission_type: EmissionType::Raw,
            divider: U16::new(divider),
            size: U32::new(1),
            num_foci: NonZeroU8::new(0),
            reserved: 0,
            sound_speed: U16::new(0),
            rep: U16::new(REP_INFINITE),
        };
        self.send(Cmd::ConfigPattern, payload.as_bytes())
    }

    fn activate_pattern_bank(
        &mut self,
        bank: PatternBank,
        mode: TransitionMode,
    ) -> DeviceErrorCode {
        let payload = ActivatePatternBankPayload {
            bank,
            transition_mode: mode,
            transition_value: U64::new(0),
        };
        self.send(Cmd::ActivatePatternBank, payload.as_bytes())
    }

    fn set_silencer(
        &mut self,
        flags: SilencerFlags,
        intensity: u16,
        phase: u16,
    ) -> DeviceErrorCode {
        let payload = SilencerPayload {
            flag: flags.bits(),
            reserved: 0,
            update_rate_intensity: U16::new(SILENCER_DEFAULT_UPDATE_RATE),
            update_rate_phase: U16::new(SILENCER_DEFAULT_UPDATE_RATE),
            completion_steps_intensity: U16::new(intensity),
            completion_steps_phase: U16::new(phase),
        };
        self.send(Cmd::SetSilencer, payload.as_bytes())
    }

    fn assert_silencer_at_default(&self) {
        assert_eq!(
            self.device.fpga().silencer_completion_steps_intensity(),
            SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY
        );
        assert_eq!(
            self.device.fpga().silencer_completion_steps_phase(),
            SILENCER_DEFAULT_COMPLETION_STEPS_PHASE
        );
    }
}

#[test]
fn strict_is_rejected_ahead_of_a_pending_sys_time_transition() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.config_modulation(B1, 100, 0), DeviceErrorCode::None);
    assert_eq!(
        host.activate_modulation_bank(B1, SYS_TIME, 1_000_000_000),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().current_mod_bank(), 0);

    assert_eq!(
        host.set_silencer(STRICT, 8, 40),
        DeviceErrorCode::InvalidSilencerSetting
    );
    host.assert_silencer_at_default();
}

#[test]
fn strict_is_rejected_while_ext_alternates_onto_a_faster_bank() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.config_modulation(B1, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, EXT, 0),
        DeviceErrorCode::None
    );

    assert_eq!(
        host.set_silencer(STRICT, 8, 40),
        DeviceErrorCode::InvalidSilencerSetting
    );
    host.assert_silencer_at_default();
}

#[test]
fn strict_is_rejected_while_ext_keeps_alternating_under_a_finite_request() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.config_modulation(B1, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, EXT, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.config_modulation(B0, 100, 0), DeviceErrorCode::None);
    assert_eq!(
        host.activate_modulation_bank(B0, SYNC_IDX, 0),
        DeviceErrorCode::None
    );

    assert_eq!(
        host.set_silencer(STRICT, 8, 40),
        DeviceErrorCode::InvalidSilencerSetting
    );
    host.assert_silencer_at_default();
}

#[test]
fn a_faster_divider_on_an_unused_bank_is_not_rejected() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.config_modulation(B1, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );

    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);
    assert_eq!(host.device.fpga().silencer_completion_steps_intensity(), 8);

    assert_eq!(
        host.config_modulation(B1, 3, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(1), 3);
}

#[test]
fn activation_onto_a_faster_bank_is_rejected_and_playback_is_unchanged() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.config_modulation(B1, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);

    assert_eq!(
        host.config_modulation(B0, 50, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B1, IMMEDIATE, 0),
        DeviceErrorCode::InvalidSilencerSetting
    );
    assert_eq!(host.device.fpga().current_mod_bank(), 0);
    assert_eq!(host.device.fpga().req_modulation_bank(), 0);
    assert_eq!(host.device.fpga().modulation_freq_div(0), 100);
    assert_eq!(host.device.fpga().modulation_freq_div(1), 5);
}

#[test]
fn fixed_update_rate_and_clear_release_the_guard() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);
    assert_eq!(
        host.config_modulation(B0, 1, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::InvalidSilencerSetting
    );

    assert_eq!(
        host.set_silencer(STRICT | FIXED_UPDATE_RATE, 8, 40),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 1);

    assert_eq!(
        host.config_modulation(B0, 0xFFFF, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.set_silencer(STRICT, 0xFFFF, 0xFFFF),
        DeviceErrorCode::None
    );
    assert_eq!(host.send(Cmd::Clear, &[]), DeviceErrorCode::None);
    host.assert_silencer_at_default();
    assert_eq!(
        host.config_modulation(B0, 1, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 1);
}

#[test]
fn config_is_not_rejected_but_the_activation_that_latches_it() {
    let mut host = Host::new();
    assert_eq!(host.set_silencer(STRICT, 10, 40), DeviceErrorCode::None);

    assert_eq!(
        host.config_modulation(B0, 9, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 0xFFFF);
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::InvalidSilencerSetting
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 0xFFFF);

    assert_eq!(
        host.config_modulation(B0, 10, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 10);
}

#[test]
fn pattern_is_guarded_on_the_larger_of_intensity_and_phase() {
    let mut host = Host::new();
    assert_eq!(host.set_silencer(STRICT, 10, 40), DeviceErrorCode::None);

    assert_eq!(
        host.config_pattern(PatternBank::B0, 20),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_pattern_bank(PatternBank::B0, IMMEDIATE),
        DeviceErrorCode::InvalidSilencerSetting
    );
    assert_eq!(host.device.fpga().pattern_freq_div(0), 0xFFFF);

    assert_eq!(
        host.config_pattern(PatternBank::B0, 40),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_pattern_bank(PatternBank::B0, IMMEDIATE),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().pattern_freq_div(0), 40);
    assert_eq!(host.device.fpga().current_pattern_bank(), 0);
}

#[test]
fn non_strict_does_not_guard_sampling() {
    let mut host = Host::new();
    assert_eq!(
        host.set_silencer(SilencerFlags::empty(), 10, 40),
        DeviceErrorCode::None
    );

    assert_eq!(
        host.config_modulation(B0, 1, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 1);
}

#[test]
fn strict_is_rejected_when_the_playing_bank_samples_too_fast() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );

    assert_eq!(
        host.set_silencer(STRICT, 8, 40),
        DeviceErrorCode::InvalidSilencerSetting
    );
    host.assert_silencer_at_default();
}

#[test]
fn strict_ignores_a_divider_that_is_written_but_not_latched() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 5, REP_INFINITE),
        DeviceErrorCode::None
    );

    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);

    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::InvalidSilencerSetting
    );
}

#[test]
fn strict_is_rejected_while_a_too_fast_divider_is_still_latched() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );

    assert_eq!(
        host.set_silencer(STRICT, 8, 40),
        DeviceErrorCode::InvalidSilencerSetting
    );

    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);
}

#[test]
fn a_finite_request_is_rejected_while_the_playing_bank_gets_too_fast() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.set_silencer(STRICT, 8, 40), DeviceErrorCode::None);

    assert_eq!(
        host.config_modulation(B0, 5, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(host.config_modulation(B1, 100, 0), DeviceErrorCode::None);
    assert_eq!(
        host.activate_modulation_bank(B1, SYS_TIME, 1_000_000_000),
        DeviceErrorCode::InvalidSilencerSetting
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 100);
    assert_eq!(host.device.fpga().req_modulation_bank(), 0);
}

#[test]
fn a_divider_equal_to_the_completion_steps_is_accepted() {
    let mut host = Host::new();
    assert_eq!(
        host.config_modulation(B0, 100, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.set_silencer(STRICT, 8, 8), DeviceErrorCode::None);

    assert_eq!(
        host.config_modulation(B0, 7, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::InvalidSilencerSetting
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 100);

    assert_eq!(
        host.config_modulation(B0, 8, REP_INFINITE),
        DeviceErrorCode::None
    );
    assert_eq!(
        host.activate_modulation_bank(B0, IMMEDIATE, 0),
        DeviceErrorCode::None
    );
    assert_eq!(host.device.fpga().modulation_freq_div(0), 8);
}

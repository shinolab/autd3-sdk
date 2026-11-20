use core::mem::offset_of;

use autd3_cpu_wire::payload::SilencerFlags;
use autd3_cpu_wire::{ModulationBank, PatternBank};
use zerocopy::FromZeros;
use zerocopy::little_endian::{U16, U64};

use crate::cmd::activate_mod_bank::ActivateModBankPayload;
use crate::cmd::activate_pattern_bank::ActivatePatternBankPayload;
use crate::cmd::cpu_config::{CpuConfig, SetCpuConfigPayload};
use crate::cmd::force_fan::ForceFanPayload;
use crate::cmd::fpga_config::SetFpgaConfigPayload;
use crate::cmd::gpio_in::GpioInPayload;
use crate::cmd::gpio_out::GpioOutPayload;
use crate::cmd::output_mask::OutputMaskPayload;
use crate::cmd::phase_corr::PhaseCorrPayload;
use crate::cmd::pwe::PwePayload;
use crate::cmd::write_pattern_fused::WritePatternFusedPayload;
use crate::fpga::{REP_INFINITE, TransitionModeKind};
use crate::fpga_params::NUM_BANKS;
use crate::proto::Cmd;
use crate::test_utils::mock::{Frame, Harness};

pub(crate) fn invalid_bank() -> u8 {
    u8::try_from(NUM_BANKS).unwrap()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FpgaSnapshot {
    ctl: std::vec::Vec<u16>,
    em_ram: std::vec::Vec<std::vec::Vec<u16>>,
    mod_ram: std::vec::Vec<std::vec::Vec<u16>>,
    latch_count: [u32; 16],
    latched: std::vec::Vec<u16>,
}

pub(crate) fn fpga_snapshot(h: &Harness) -> FpgaSnapshot {
    FpgaSnapshot {
        ctl: h.port.ctl.to_vec(),
        em_ram: h.port.em_ram.clone(),
        mod_ram: h.port.mod_ram.clone(),
        latch_count: h.port.latch_count,
        latched: h.port.latched.to_vec(),
    }
}

pub(crate) fn assert_fpga_unchanged(before: &FpgaSnapshot, h: &Harness) {
    let after = fpga_snapshot(h);
    assert!(
        *before == after,
        "rejected frame must not touch RAM, controller registers, latches or effective settings"
    );
}

pub(crate) fn write_foci_buffer(bank: u8, offset_words: u32, words: &[u16]) -> Frame {
    let payload: std::vec::Vec<u8> = [bank]
        .into_iter()
        .chain(offset_words.to_le_bytes())
        .chain(words.iter().flat_map(|w| w.to_le_bytes()))
        .collect();
    Frame::from_payload(Cmd::WriteFociBuffer, payload.as_slice())
}

fn interleave<'a>(phases: &'a [u8], intensities: &'a [u8]) -> impl Iterator<Item = u8> + 'a {
    assert_eq!(phases.len(), intensities.len());
    phases
        .iter()
        .zip(intensities)
        .flat_map(|(&phase, &intensity)| [phase, intensity])
}

pub(crate) fn write_pattern_raw(bank: u8, index: u16, phases: &[u8], intensities: &[u8]) -> Frame {
    write_pattern_raw_multi(bank, index, &[(phases, intensities)])
}

pub(crate) fn write_pattern_raw_multi(bank: u8, index: u16, slots: &[(&[u8], &[u8])]) -> Frame {
    let payload: std::vec::Vec<u8> = [bank, u8::try_from(slots.len()).unwrap()]
        .into_iter()
        .chain(index.to_le_bytes())
        .chain(
            slots
                .iter()
                .flat_map(|(phases, intensities)| interleave(phases, intensities)),
        )
        .collect();
    Frame::from_payload(Cmd::WritePatternRaw, payload.as_slice())
}

pub(crate) fn write_pattern_fused(
    bank: u8,
    transition_mode: TransitionModeKind,
    transition_value: u64,
    phases: &[u8],
    intensities: &[u8],
) -> Frame {
    let header = WritePatternFusedPayload {
        bank: PatternBank::B0,
        transition_mode,
        transition_value: U64::new(transition_value),
    };
    let data: std::vec::Vec<u8> = interleave(phases, intensities).collect();
    let mut f = Frame::from_parts(Cmd::WritePatternFused, &header, &data);
    f.set_payload_byte(offset_of!(WritePatternFusedPayload, bank), bank);
    f
}

pub(crate) fn write_pattern_phase(
    bank: u8,
    index: u16,
    depth: u8,
    count: u8,
    intensity: u8,
    data: &[u8],
) -> Frame {
    let payload: std::vec::Vec<u8> = [bank, depth, count, intensity]
        .into_iter()
        .chain(index.to_le_bytes())
        .chain(data.iter().copied())
        .collect();
    Frame::from_payload(Cmd::WritePatternPhase, payload.as_slice())
}

pub(crate) fn write_mod_buffer(bank: u8, offset: u32, data: &[u8]) -> Frame {
    let payload: std::vec::Vec<u8> = [bank]
        .into_iter()
        .chain(offset.to_le_bytes())
        .chain(data.iter().copied())
        .collect();
    Frame::from_payload(Cmd::WriteModulationBuffer, payload.as_slice())
}

pub(crate) fn config_mod(bank: u8, divider: u16, size: u32) -> Frame {
    config_mod_rep(bank, divider, size, REP_INFINITE)
}

pub(crate) fn config_mod_rep(bank: u8, divider: u16, size: u32, rep: u16) -> Frame {
    let payload: std::vec::Vec<u8> = [bank]
        .into_iter()
        .chain(divider.to_le_bytes())
        .chain(size.to_le_bytes())
        .chain(rep.to_le_bytes())
        .collect();
    Frame::from_payload(Cmd::ConfigModulation, payload.as_slice())
}

pub(crate) fn config_pattern(
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
) -> Frame {
    config_pattern_rep(
        bank,
        emission_type,
        divider,
        size,
        num_foci,
        sound_speed,
        REP_INFINITE,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn config_pattern_rep(
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Frame {
    let payload: std::vec::Vec<u8> = [bank, emission_type]
        .into_iter()
        .chain(divider.to_le_bytes())
        .chain(size.to_le_bytes())
        .chain([num_foci])
        .chain(sound_speed.to_le_bytes())
        .chain(rep.to_le_bytes())
        .collect();
    Frame::from_payload(Cmd::ConfigPattern, payload.as_slice())
}

pub(crate) fn activate_pattern_bank(
    bank: u8,
    transition_mode: TransitionModeKind,
    transition_value: u64,
) -> Frame {
    let p = ActivatePatternBankPayload {
        bank: PatternBank::B0,
        transition_mode,
        transition_value: U64::new(transition_value),
    };
    let mut f = Frame::from_payload(Cmd::ActivatePatternBank, &p);
    f.set_payload_byte(offset_of!(ActivatePatternBankPayload, bank), bank);
    f
}

pub(crate) fn activate_mod_bank(
    bank: u8,
    transition_mode: TransitionModeKind,
    transition_value: u64,
) -> Frame {
    let p = ActivateModBankPayload {
        bank: ModulationBank::B0,
        transition_mode,
        transition_value: U64::new(transition_value),
    };
    let mut f = Frame::from_payload(Cmd::ActivateModulationBank, &p);
    f.set_payload_byte(offset_of!(ActivateModBankPayload, bank), bank);
    f
}

pub(crate) fn set_cpu_config(config: &CpuConfig) -> Frame {
    Frame::from_payload(
        Cmd::SetCpuConfig,
        &SetCpuConfigPayload::encode(config).unwrap(),
    )
}

pub(crate) fn set_silencer(
    flag: SilencerFlags,
    update_rate_intensity: u16,
    update_rate_phase: u16,
    completion_steps_intensity: u16,
    completion_steps_phase: u16,
) -> Frame {
    let payload: std::vec::Vec<u8> = [flag.bits()]
        .into_iter()
        .chain(update_rate_intensity.to_le_bytes())
        .chain(update_rate_phase.to_le_bytes())
        .chain(completion_steps_intensity.to_le_bytes())
        .chain(completion_steps_phase.to_le_bytes())
        .collect();
    Frame::from_payload(Cmd::SetSilencer, payload.as_slice())
}

pub(crate) fn force_fan(value: u8) -> Frame {
    let mut f = Frame::from_payload(Cmd::ForceFan, &ForceFanPayload { value: false });
    f.set_payload_byte(offset_of!(ForceFanPayload, value), value);
    f
}

pub(crate) fn fpga_config(sync_phase_lock: u8, gains: [u16; 3]) -> Frame {
    let mut f = Frame::from_payload(
        Cmd::SetFpgaConfig,
        &SetFpgaConfigPayload {
            sync_phase_lock: false,
            sync_kp: gains[0].into(),
            sync_ki: gains[1].into(),
            sync_kf: gains[2].into(),
        },
    );
    f.set_payload_byte(
        offset_of!(SetFpgaConfigPayload, sync_phase_lock),
        sync_phase_lock,
    );
    f
}

pub(crate) fn gpio_in(values: [u8; 4]) -> Frame {
    let p = GpioInPayload { values: [false; 4] };
    let mut f = Frame::from_payload(Cmd::EmulateGpioIn, &p);
    for (i, value) in values.into_iter().enumerate() {
        f.set_payload_byte(offset_of!(GpioInPayload, values) + i, value);
    }
    f
}

pub(crate) fn phase_corr(phases: &[u8]) -> Frame {
    let mut p = PhaseCorrPayload::new_zeroed();
    p.data[..phases.len()].copy_from_slice(phases);
    Frame::from_payload(Cmd::SetPhaseCorrection, &p)
}

pub(crate) fn output_mask(mask: &[bool]) -> Frame {
    let mut p = OutputMaskPayload::new_zeroed();
    for (i, on) in mask.iter().enumerate() {
        let word = &mut p.words[i / 16];
        word.set(word.get() | (u16::from(*on) << (i % 16)));
    }
    Frame::from_payload(Cmd::SetOutputMask, &p)
}

pub(crate) fn pwe(table: &[u16]) -> Frame {
    let mut p = PwePayload::new_zeroed();
    for (i, w) in table.iter().enumerate() {
        p.table[i] = U16::new(*w);
    }
    Frame::from_payload(Cmd::SetPulseWidthTable, &p)
}

pub(crate) fn gpio_out(values: &[u64]) -> Frame {
    let mut p = GpioOutPayload::new_zeroed();
    for (i, v) in values.iter().enumerate() {
        p.values[i] = U64::new(*v);
    }
    Frame::from_payload(Cmd::SetGpioOut, &p)
}

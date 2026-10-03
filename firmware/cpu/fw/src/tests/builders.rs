use core::mem::offset_of;

use autd3_cpu_wire::payload::{EmissionType, PhaseDepth};
use autd3_cpu_wire::{ModulationBank, PatternBank};
use zerocopy::FromZeros;
use zerocopy::little_endian::{U16, U32, U64};

use crate::cmd::activate_mod_bank::ActivateModBankPayload;
use crate::cmd::activate_pattern_bank::ActivatePatternBankPayload;
use crate::cmd::config_mod::ConfigModPayload;
use crate::cmd::config_pattern::ConfigPatternPayload;
use crate::cmd::force_fan::ForceFanPayload;
use crate::cmd::gpio_in::GpioInPayload;
use crate::cmd::gpio_out::GpioOutPayload;
use crate::cmd::output_mask::OutputMaskPayload;
use crate::cmd::phase_corr::PhaseCorrPayload;
use crate::cmd::pwe::PwePayload;
use crate::cmd::silencer::SilencerPayload;
use crate::cmd::write_foci::WriteFociPayload;
use crate::cmd::write_mod::WriteModPayload;
use crate::cmd::write_pattern_phase::WritePatternPhasePayload;
use crate::cmd::write_pattern_raw::WritePatternRawPayload;
use crate::fpga::{REP_INFINITE, TransitionMode};
use crate::params::NUM_BANKS;
use crate::proto::Cmd;
use crate::tests::mock::{Frame, Harness};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FpgaSnapshot {
    ctl: std::vec::Vec<u16>,
    em_ram: std::vec::Vec<std::vec::Vec<u16>>,
    mod_ram: std::vec::Vec<std::vec::Vec<u16>>,
    latch_count: [u32; 16],
    pattern_div: std::vec::Vec<u16>,
    mod_div: std::vec::Vec<u16>,
}

pub(crate) fn fpga_snapshot(h: &Harness) -> FpgaSnapshot {
    let banks = 0..u8::try_from(NUM_BANKS).unwrap();
    FpgaSnapshot {
        ctl: h.port.ctl.to_vec(),
        em_ram: h.port.em_ram.clone(),
        mod_ram: h.port.mod_ram.clone(),
        latch_count: h.port.latch_count,
        pattern_div: banks
            .clone()
            .map(|b| h.cpu.silencer.pattern_freq_div[usize::from(b)].get())
            .collect(),
        mod_div: banks
            .map(|b| h.cpu.silencer.mod_freq_div[usize::from(b)].get())
            .collect(),
    }
}

pub(crate) fn assert_fpga_unchanged(before: &FpgaSnapshot, h: &Harness) {
    let after = fpga_snapshot(h);
    assert!(
        *before == after,
        "rejected frame must not touch RAM, controller registers, latches or silencer mirrors"
    );
}

fn words_to_bytes(words: &[u16]) -> std::vec::Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

pub(crate) fn write_foci_buffer(seq: u8, bank: u8, offset_words: u32, words: &[u16]) -> Frame {
    let header = WriteFociPayload {
        bank: PatternBank::B0,
        reserved: 0,
        offset: U32::new(offset_words),
    };
    let mut f = Frame::from_parts(seq, Cmd::WriteFociBuffer, &header, &words_to_bytes(words));
    f.set_payload_byte(offset_of!(WriteFociPayload, bank), bank);
    f
}

pub(crate) fn write_pattern_raw(
    seq: u8,
    bank: u8,
    index: u16,
    phases: &[u8],
    intensities: &[u8],
) -> Frame {
    write_pattern_raw_multi(seq, bank, index, &[(phases, intensities)])
}

pub(crate) fn write_pattern_raw_multi(
    seq: u8,
    bank: u8,
    index: u16,
    slots: &[(&[u8], &[u8])],
) -> Frame {
    let header = WritePatternRawPayload {
        bank: PatternBank::B0,
        count: u8::try_from(slots.len()).unwrap(),
        index: U16::new(index),
    };
    let data: std::vec::Vec<u8> = slots
        .iter()
        .flat_map(|(phases, intensities)| phases.iter().chain(intensities.iter()).copied())
        .collect();
    let mut f = Frame::from_parts(seq, Cmd::WritePatternRaw, &header, &data);
    f.set_payload_byte(offset_of!(WritePatternRawPayload, bank), bank);
    f
}

pub(crate) fn write_pattern_phase(
    seq: u8,
    bank: u8,
    index: u16,
    depth: u8,
    count: u8,
    intensity: u8,
    data: &[u8],
) -> Frame {
    let header = WritePatternPhasePayload {
        bank: PatternBank::B0,
        depth: PhaseDepth::Bits8,
        count,
        intensity,
        index: U16::new(index),
    };
    let mut f = Frame::from_parts(seq, Cmd::WritePatternPhase, &header, data);
    f.set_payload_byte(offset_of!(WritePatternPhasePayload, bank), bank);
    f.set_payload_byte(offset_of!(WritePatternPhasePayload, depth), depth);
    f
}

pub(crate) fn write_mod_buffer(seq: u8, bank: u8, offset: u32, data: &[u8]) -> Frame {
    let header = WriteModPayload {
        bank: ModulationBank::B0,
        reserved: 0,
        offset: U32::new(offset),
    };
    let mut f = Frame::from_parts(seq, Cmd::WriteModulationBuffer, &header, data);
    f.set_payload_byte(offset_of!(WriteModPayload, bank), bank);
    f
}

pub(crate) fn config_mod(seq: u8, bank: u8, divider: u16, size: u32) -> Frame {
    config_mod_rep(seq, bank, divider, size, REP_INFINITE)
}

pub(crate) fn config_mod_rep(seq: u8, bank: u8, divider: u16, size: u32, rep: u16) -> Frame {
    let p = ConfigModPayload {
        bank: ModulationBank::B0,
        reserved: 0,
        divider: U16::new(divider),
        size: U32::new(size),
        rep: U16::new(rep),
    };
    let mut f = Frame::from_payload(seq, Cmd::ConfigModulation, &p);
    f.set_payload_byte(offset_of!(ConfigModPayload, bank), bank);
    f
}

pub(crate) fn config_pattern(
    seq: u8,
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
) -> Frame {
    config_pattern_rep(
        seq,
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
    seq: u8,
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Frame {
    let p = ConfigPatternPayload {
        bank: PatternBank::B0,
        emission_type: EmissionType::Foci,
        divider: U16::new(divider),
        size: U32::new(size),
        num_foci,
        reserved: 0,
        sound_speed: U16::new(sound_speed),
        rep: U16::new(rep),
    };
    let mut f = Frame::from_payload(seq, Cmd::ConfigPattern, &p);
    f.set_payload_byte(offset_of!(ConfigPatternPayload, bank), bank);
    f.set_payload_byte(
        offset_of!(ConfigPatternPayload, emission_type),
        emission_type,
    );
    f
}

pub(crate) fn activate_pattern_bank(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
) -> Frame {
    let p = ActivatePatternBankPayload {
        bank: PatternBank::B0,
        transition_mode,
        transition_value: U64::new(transition_value),
        margin_ns: U32::new(0),
    };
    let mut f = Frame::from_payload(seq, Cmd::ActivatePatternBank, &p);
    f.set_payload_byte(offset_of!(ActivatePatternBankPayload, bank), bank);
    f
}

pub(crate) fn activate_mod_bank(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
) -> Frame {
    activate_mod_bank_with_margin(seq, bank, transition_mode, transition_value, 0)
}

pub(crate) fn activate_mod_bank_with_margin(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
    margin_ns: u32,
) -> Frame {
    let p = ActivateModBankPayload {
        bank: ModulationBank::B0,
        transition_mode,
        transition_value: U64::new(transition_value),
        margin_ns: U32::new(margin_ns),
    };
    let mut f = Frame::from_payload(seq, Cmd::ActivateModulationBank, &p);
    f.set_payload_byte(offset_of!(ActivateModBankPayload, bank), bank);
    f
}

pub(crate) fn set_silencer(
    seq: u8,
    flag: u8,
    update_rate_intensity: u16,
    update_rate_phase: u16,
    completion_steps_intensity: u16,
    completion_steps_phase: u16,
) -> Frame {
    let mut p = SilencerPayload::new_zeroed();
    p.flag = flag;
    p.update_rate_intensity = U16::new(update_rate_intensity);
    p.update_rate_phase = U16::new(update_rate_phase);
    p.completion_steps_intensity = U16::new(completion_steps_intensity);
    p.completion_steps_phase = U16::new(completion_steps_phase);
    Frame::from_payload(seq, Cmd::SetSilencer, &p)
}

pub(crate) fn force_fan(seq: u8, value: u8) -> Frame {
    let mut f = Frame::from_payload(seq, Cmd::ForceFan, &ForceFanPayload { value: false });
    f.set_payload_byte(offset_of!(ForceFanPayload, value), value);
    f
}

pub(crate) fn gpio_in(seq: u8, values: [u8; 4]) -> Frame {
    let p = GpioInPayload {
        gpio_in_0: false,
        gpio_in_1: false,
        gpio_in_2: false,
        gpio_in_3: false,
    };
    let mut f = Frame::from_payload(seq, Cmd::EmulateGpioIn, &p);
    for (i, value) in values.into_iter().enumerate() {
        f.set_payload_byte(offset_of!(GpioInPayload, gpio_in_0) + i, value);
    }
    f
}

pub(crate) fn phase_corr(seq: u8, phases: &[u8]) -> Frame {
    let mut p = PhaseCorrPayload::new_zeroed();
    p.data[..phases.len()].copy_from_slice(phases);
    Frame::from_payload(seq, Cmd::SetPhaseCorrection, &p)
}

pub(crate) fn output_mask(seq: u8, mask: &[bool]) -> Frame {
    let mut p = OutputMaskPayload::new_zeroed();
    for (i, on) in mask.iter().enumerate() {
        p.data[i] = u8::from(*on);
    }
    Frame::from_payload(seq, Cmd::SetOutputMask, &p)
}

pub(crate) fn pwe(seq: u8, table: &[u16]) -> Frame {
    let mut p = PwePayload::new_zeroed();
    for (i, w) in table.iter().enumerate() {
        p.table[i] = U16::new(*w);
    }
    Frame::from_payload(seq, Cmd::SetPulseWidthTable, &p)
}

pub(crate) fn gpio_out(seq: u8, values: &[u64]) -> Frame {
    let mut p = GpioOutPayload::new_zeroed();
    for (i, v) in values.iter().enumerate() {
        p.values[i] = U64::new(*v);
    }
    Frame::from_payload(seq, Cmd::SetGpioOut, &p)
}

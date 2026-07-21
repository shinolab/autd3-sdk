use zerocopy::FromZeros;
use zerocopy::little_endian::{U16, U32, U64};

use crate::cmd::change_mod_bank::ChangeModBankPayload;
use crate::cmd::change_pattern_bank::ChangePatternBankPayload;
use crate::cmd::config_mod::ConfigModPayload;
use crate::cmd::config_pattern::ConfigPatternPayload;
use crate::cmd::force_fan::ForceFanPayload;
use crate::cmd::gpio_in::GpioInPayload;
use crate::cmd::gpio_out::GpioOutPayload;
use crate::cmd::output_mask::OutputMaskPayload;
use crate::cmd::phase_corr::PhaseCorrPayload;
use crate::cmd::pwe::PwePayload;
use crate::cmd::silencer::SilencerPayload;
use crate::cmd::write_mod::WriteModPayload;
use crate::cmd::write_mod_fused::WriteModulationFusedPayload;
use crate::cmd::write_pattern::WritePatternPayload;
use crate::cmd::write_pattern_compressed::WritePatternCompressedPayload;
use crate::cmd::write_pattern_fused::WritePatternFusedPayload;
use crate::cmd::xor_hash::XorHashPayload;
use crate::fpga::{REP_INFINITE, TransitionMode};
use crate::proto::Cmd;
use crate::tests::mock::Frame;

fn words_to_bytes(words: &[u16]) -> std::vec::Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

pub(crate) fn xor_hash_ok(seq: u8, sleep_ms: u16, data: &[u8]) -> Frame {
    let header = XorHashPayload {
        sleep_ms: U16::new(sleep_ms),
        data_len: U16::new((data.len() + 1) as u16),
    };
    let mut payload = data.to_vec();
    payload.push(data.iter().fold(0u8, |h, b| h ^ b));
    Frame::from_parts(seq, Cmd::XorHash, &header, &payload)
}

pub(crate) fn xor_hash_bad(seq: u8, data: &[u8]) -> Frame {
    let header = XorHashPayload {
        sleep_ms: U16::new(0),
        data_len: U16::new(data.len() as u16),
    };
    Frame::from_parts(seq, Cmd::XorHash, &header, data)
}

pub(crate) fn xor_hash_corrupted(seq: u8, sleep_ms: u16, data: &[u8]) -> Frame {
    let header = XorHashPayload {
        sleep_ms: U16::new(sleep_ms),
        data_len: U16::new((data.len() + 1) as u16),
    };
    let mut payload = data.to_vec();
    payload.push(data.iter().fold(0u8, |h, b| h ^ b));
    payload[0] ^= 0xFF;
    Frame::from_parts(seq, Cmd::XorHash, &header, &payload)
}

pub(crate) fn write_pattern_buffer(seq: u8, bank: u8, offset_words: u32, words: &[u16]) -> Frame {
    let header = WritePatternPayload {
        bank,
        reserved: 0,
        offset: U32::new(offset_words),
        data_len: U16::new((words.len() * 2) as u16),
    };
    Frame::from_parts(
        seq,
        Cmd::WritePatternBuffer,
        &header,
        &words_to_bytes(words),
    )
}

pub(crate) fn write_pattern_compressed(
    seq: u8,
    bank: u8,
    offset_words: u32,
    format: u8,
    count: u8,
    words: &[u16],
) -> Frame {
    let header = WritePatternCompressedPayload {
        bank,
        format,
        count,
        reserved: 0,
        offset: U32::new(offset_words),
    };
    Frame::from_parts(
        seq,
        Cmd::WritePatternCompressed,
        &header,
        &words_to_bytes(words),
    )
}

pub(crate) fn write_mod_buffer(seq: u8, bank: u8, offset: u32, data: &[u8]) -> Frame {
    let header = WriteModPayload {
        bank,
        reserved: 0,
        offset: U32::new(offset),
        data_len: U16::new(data.len() as u16),
    };
    Frame::from_parts(seq, Cmd::WriteModulationBuffer, &header, data)
}

pub(crate) fn config_mod(seq: u8, bank: u8, divider: u16, size: u32) -> Frame {
    config_mod_rep(seq, bank, divider, size, REP_INFINITE)
}

pub(crate) fn config_mod_rep(seq: u8, bank: u8, divider: u16, size: u32, rep: u16) -> Frame {
    let mut p = ConfigModPayload::new_zeroed();
    p.bank = bank;
    p.divider = U16::new(divider);
    p.size = U32::new(size);
    p.rep = U16::new(rep);
    Frame::from_payload(seq, Cmd::ConfigModulation, &p)
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
    let mut p = ConfigPatternPayload::new_zeroed();
    p.bank = bank;
    p.emission_type = emission_type;
    p.divider = U16::new(divider);
    p.size = U32::new(size);
    p.num_foci = num_foci;
    p.sound_speed = U16::new(sound_speed);
    p.rep = U16::new(rep);
    Frame::from_payload(seq, Cmd::ConfigPattern, &p)
}

pub(crate) fn change_pattern_bank(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
) -> Frame {
    let mut p = ChangePatternBankPayload::new_zeroed();
    p.bank = bank;
    p.transition_mode = transition_mode as u8;
    p.transition_value = U64::new(transition_value);
    Frame::from_payload(seq, Cmd::ChangePatternBank, &p)
}

pub(crate) fn change_mod_bank(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
) -> Frame {
    change_mod_bank_with_margin(seq, bank, transition_mode, transition_value, 0)
}

pub(crate) fn change_mod_bank_with_margin(
    seq: u8,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
    margin_ns: u32,
) -> Frame {
    let mut p = ChangeModBankPayload::new_zeroed();
    p.bank = bank;
    p.transition_mode = transition_mode as u8;
    p.transition_value = U64::new(transition_value);
    p.margin_ns = U32::new(margin_ns);
    Frame::from_payload(seq, Cmd::ChangeModulationBank, &p)
}

pub(crate) struct FusedPattern {
    pub(crate) bank: u8,
    pub(crate) emission_type: u8,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) num_foci: u8,
    pub(crate) sound_speed: u16,
    pub(crate) rep: u16,
    pub(crate) transition_mode: TransitionMode,
    pub(crate) transition_value: u64,
    pub(crate) margin_ns: u32,
}

impl FusedPattern {
    pub(crate) fn raw(bank: u8, divider: u16, size: u32) -> Self {
        Self {
            bank,
            emission_type: crate::params::EMISSION_TYPE_RAW,
            divider,
            size,
            num_foci: 0,
            sound_speed: 0,
            rep: REP_INFINITE,
            transition_mode: TransitionMode::Immediate,
            transition_value: 0,
            margin_ns: 0,
        }
    }
}

pub(crate) fn write_pattern_fused(seq: u8, f: &FusedPattern, words: &[u16]) -> Frame {
    let header = WritePatternFusedPayload {
        bank: f.bank,
        emission_type: f.emission_type,
        divider: U16::new(f.divider),
        size: U32::new(f.size),
        num_foci: f.num_foci,
        transition_mode: f.transition_mode as u8,
        sound_speed: U16::new(f.sound_speed),
        rep: U16::new(f.rep),
        data_len: U16::new((words.len() * 2) as u16),
        transition_value: U64::new(f.transition_value),
        margin_ns: U32::new(f.margin_ns),
        reserved: U32::new(0),
    };
    Frame::from_parts(seq, Cmd::WritePatternFused, &header, &words_to_bytes(words))
}

pub(crate) struct FusedMod {
    pub(crate) bank: u8,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) rep: u16,
    pub(crate) transition_mode: TransitionMode,
    pub(crate) transition_value: u64,
    pub(crate) margin_ns: u32,
}

impl FusedMod {
    pub(crate) fn new(bank: u8, divider: u16, size: u32) -> Self {
        Self {
            bank,
            divider,
            size,
            rep: REP_INFINITE,
            transition_mode: TransitionMode::Immediate,
            transition_value: 0,
            margin_ns: 0,
        }
    }
}

pub(crate) fn write_mod_fused(seq: u8, f: &FusedMod, data: &[u8]) -> Frame {
    let header = WriteModulationFusedPayload {
        bank: f.bank,
        transition_mode: f.transition_mode as u8,
        divider: U16::new(f.divider),
        size: U32::new(f.size),
        rep: U16::new(f.rep),
        data_len: U16::new(data.len() as u16),
        transition_value: U64::new(f.transition_value),
        margin_ns: U32::new(f.margin_ns),
    };
    Frame::from_parts(seq, Cmd::WriteModulationFused, &header, data)
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
    let mut p = ForceFanPayload::new_zeroed();
    p.value = value;
    Frame::from_payload(seq, Cmd::ForceFan, &p)
}

pub(crate) fn gpio_in(seq: u8, flag: u8) -> Frame {
    let mut p = GpioInPayload::new_zeroed();
    p.flag = flag;
    Frame::from_payload(seq, Cmd::EmulateGpioIn, &p)
}

pub(crate) fn phase_corr(seq: u8, phases: &[u8]) -> Frame {
    let mut p = PhaseCorrPayload::new_zeroed();
    p.data[..phases.len()].copy_from_slice(phases);
    Frame::from_payload(seq, Cmd::SetPhaseCorrection, &p)
}

pub(crate) fn output_mask(seq: u8, words: &[u16]) -> Frame {
    let mut p = OutputMaskPayload::new_zeroed();
    for (i, w) in words.iter().enumerate() {
        p.data[i] = U16::new(*w);
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

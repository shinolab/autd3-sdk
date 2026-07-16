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
use crate::cmd::write_pattern::WritePatternPayload;
use crate::cmd::write_pattern_compressed::WritePatternCompressedPayload;
use crate::cmd::xor_hash::XorHashPayload;
use crate::fpga::{REP_INFINITE, TransitionMode};
use crate::proto::Cmd;
use crate::tests::mock::Frame;

pub(crate) fn xor_hash_ok(seq: u8, sleep_ms: u16, data: &[u8]) -> Frame {
    let mut p = XorHashPayload::new_zeroed();
    p.sleep_ms = U16::new(sleep_ms);
    p.data_len = U16::new((data.len() + 1) as u16);
    p.data[..data.len()].copy_from_slice(data);
    p.data[data.len()] = data.iter().fold(0u8, |h, b| h ^ b);
    Frame::from_payload(seq, Cmd::XorHash, &p)
}

pub(crate) fn xor_hash_bad(seq: u8, data: &[u8]) -> Frame {
    let mut p = XorHashPayload::new_zeroed();
    p.data_len = U16::new(data.len() as u16);
    p.data[..data.len()].copy_from_slice(data);
    Frame::from_payload(seq, Cmd::XorHash, &p)
}

pub(crate) fn xor_hash_corrupted(seq: u8, sleep_ms: u16, data: &[u8]) -> Frame {
    let mut p = XorHashPayload::new_zeroed();
    p.sleep_ms = U16::new(sleep_ms);
    p.data_len = U16::new((data.len() + 1) as u16);
    p.data[..data.len()].copy_from_slice(data);
    p.data[data.len()] = data.iter().fold(0u8, |h, b| h ^ b);
    p.data[0] ^= 0xFF;
    Frame::from_payload(seq, Cmd::XorHash, &p)
}

pub(crate) fn write_pattern_buffer(seq: u8, bank: u8, offset_words: u32, words: &[u16]) -> Frame {
    let mut p = WritePatternPayload::new_zeroed();
    p.bank = bank;
    p.offset = U32::new(offset_words);
    p.data_len = U16::new((words.len() * 2) as u16);
    for (i, w) in words.iter().enumerate() {
        p.data[2 * i..2 * i + 2].copy_from_slice(&w.to_le_bytes());
    }
    Frame::from_payload(seq, Cmd::WritePatternBuffer, &p)
}

pub(crate) fn write_pattern_compressed(
    seq: u8,
    bank: u8,
    offset_words: u32,
    format: u8,
    count: u8,
    words: &[u16],
) -> Frame {
    let mut p = WritePatternCompressedPayload::new_zeroed();
    p.bank = bank;
    p.format = format;
    p.count = count;
    p.offset = U32::new(offset_words);
    for (i, w) in words.iter().enumerate() {
        p.data[2 * i..2 * i + 2].copy_from_slice(&w.to_le_bytes());
    }
    Frame::from_payload(seq, Cmd::WritePatternCompressed, &p)
}

pub(crate) fn write_mod_buffer(seq: u8, bank: u8, offset: u32, data: &[u8]) -> Frame {
    let mut p = WriteModPayload::new_zeroed();
    p.bank = bank;
    p.offset = U32::new(offset);
    p.data_len = U16::new(data.len() as u16);
    p.data[..data.len()].copy_from_slice(data);
    Frame::from_payload(seq, Cmd::WriteModBuffer, &p)
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
    Frame::from_payload(seq, Cmd::ConfigMod, &p)
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
    Frame::from_payload(seq, Cmd::ChangeModBank, &p)
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
    Frame::from_payload(seq, Cmd::SetPhaseCorr, &p)
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
    Frame::from_payload(seq, Cmd::SetPwe, &p)
}

pub(crate) fn gpio_out(seq: u8, values: &[u64]) -> Frame {
    let mut p = GpioOutPayload::new_zeroed();
    for (i, v) in values.iter().enumerate() {
        p.values[i] = U64::new(*v);
    }
    Frame::from_payload(seq, Cmd::SetGpioOut, &p)
}

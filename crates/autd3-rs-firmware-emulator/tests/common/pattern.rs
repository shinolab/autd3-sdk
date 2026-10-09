use core::num::NonZeroU8;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{
    ActivatePatternBankPayload, ConfigPatternPayload, EmissionType, TransitionMode,
};
use zerocopy::IntoBytes;
use zerocopy::little_endian::{U16, U32, U64};

const DIVIDER: u16 = 512;

pub fn config_pattern(
    bank: PatternBank,
    emission_type: EmissionType,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Vec<u8> {
    ConfigPatternPayload {
        bank,
        emission_type,
        divider: U16::new(DIVIDER),
        size: U32::new(size),
        num_foci: NonZeroU8::new(num_foci),
        reserved: 0,
        sound_speed: U16::new(sound_speed),
        rep: U16::new(rep),
    }
    .as_bytes()
    .to_vec()
}

pub fn activate_pattern_bank(bank: PatternBank, mode: TransitionMode) -> Vec<u8> {
    ActivatePatternBankPayload {
        bank,
        transition_mode: mode,
        transition_value: U64::new(0),
    }
    .as_bytes()
    .to_vec()
}

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::payload::{ActivatePatternBankPayload, EmissionType, TransitionModeKind};
use zerocopy::IntoBytes;
use zerocopy::little_endian::U64;

pub(crate) fn config_pattern(
    bank: PatternBank,
    emission_type: EmissionType,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Vec<u8> {
    [bank.as_u8(), emission_type.as_u8()]
        .into_iter()
        .chain(512u16.to_le_bytes())
        .chain(size.to_le_bytes())
        .chain([num_foci])
        .chain(sound_speed.to_le_bytes())
        .chain(rep.to_le_bytes())
        .collect()
}

pub(crate) fn activate_pattern_bank(bank: PatternBank, mode: TransitionModeKind) -> Vec<u8> {
    ActivatePatternBankPayload {
        bank,
        transition_mode: mode,
        transition_value: U64::new(0),
    }
    .as_bytes()
    .to_vec()
}

use autd3_cpu_wire::ModulationBank;
use autd3_cpu_wire::payload::{ActivateModBankPayload, TransitionModeKind};
use zerocopy::IntoBytes;
use zerocopy::little_endian::U64;

pub(crate) fn config_modulation(
    bank: ModulationBank,
    divider: u16,
    size: u32,
    rep: u16,
) -> Vec<u8> {
    [bank.as_u8()]
        .into_iter()
        .chain(divider.to_le_bytes())
        .chain(size.to_le_bytes())
        .chain(rep.to_le_bytes())
        .collect()
}

pub(crate) fn activate_modulation_bank(
    bank: ModulationBank,
    mode: TransitionModeKind,
    value: u64,
) -> Vec<u8> {
    ActivateModBankPayload {
        bank,
        transition_mode: mode,
        transition_value: U64::new(value),
    }
    .as_bytes()
    .to_vec()
}

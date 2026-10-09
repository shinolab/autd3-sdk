use autd3_cpu_wire::ModulationBank;
use autd3_cpu_wire::payload::{ActivateModBankPayload, ConfigModPayload, TransitionMode};
use zerocopy::IntoBytes;
use zerocopy::little_endian::{U16, U32, U64};

pub fn config_modulation(bank: ModulationBank, divider: u16, size: u32, rep: u16) -> Vec<u8> {
    ConfigModPayload {
        bank,
        reserved: 0,
        divider: U16::new(divider),
        size: U32::new(size),
        rep: U16::new(rep),
    }
    .as_bytes()
    .to_vec()
}

pub fn activate_modulation_bank(bank: ModulationBank, mode: TransitionMode, value: u64) -> Vec<u8> {
    ActivateModBankPayload {
        bank,
        transition_mode: mode,
        transition_value: U64::new(value),
    }
    .as_bytes()
    .to_vec()
}

use zerocopy::little_endian::U64;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{TransitionMode, try_read_exact};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ActivateModBankPayload {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
    pub transition_value: U64,
}

impl ActivateModBankPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, transition_value) == 2);
const _: () = assert!(core::mem::size_of::<ActivateModBankPayload>() == 10);

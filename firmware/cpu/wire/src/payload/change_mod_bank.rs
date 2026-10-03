use zerocopy::little_endian::{U32, U64};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{TransitionMode, try_read_exact};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ChangeModBankPayload {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
    pub transition_value: U64,
    pub margin_ns: U32,
}

impl ChangeModBankPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, transition_value) == 2);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, margin_ns) == 10);
const _: () = assert!(core::mem::size_of::<ChangeModBankPayload>() == 14);

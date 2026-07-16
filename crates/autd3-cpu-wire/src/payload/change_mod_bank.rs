use zerocopy::little_endian::{U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ChangeModBankPayload {
    pub bank: u8,
    pub transition_mode: u8,
    pub transition_value: U64,
    pub margin_ns: U32,
}

const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, transition_value) == 2);
const _: () = assert!(core::mem::offset_of!(ChangeModBankPayload, margin_ns) == 10);
const _: () = assert!(core::mem::size_of::<ChangeModBankPayload>() == 14);

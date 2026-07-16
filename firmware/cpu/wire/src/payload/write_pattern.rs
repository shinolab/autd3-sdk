use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternPayload {
    pub bank: u8,
    pub reserved: u8,
    pub offset: U32,
    pub data_len: U16,
}

const _: () = assert!(core::mem::offset_of!(WritePatternPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternPayload, offset) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternPayload, data_len) == 6);
const _: () = assert!(core::mem::size_of::<WritePatternPayload>() == 8);

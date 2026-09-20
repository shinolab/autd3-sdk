use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteFociPayload {
    pub bank: u8,
    pub reserved: u8,
    pub offset: U32,
    pub data_len: U16,
}

const _: () = assert!(core::mem::offset_of!(WriteFociPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteFociPayload, offset) == 2);
const _: () = assert!(core::mem::offset_of!(WriteFociPayload, data_len) == 6);
const _: () = assert!(core::mem::size_of::<WriteFociPayload>() == 8);

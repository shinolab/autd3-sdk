use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    pub bank: u8,
    pub reserved: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
}

const _: () = assert!(core::mem::offset_of!(ConfigModPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, rep) == 8);
const _: () = assert!(core::mem::size_of::<ConfigModPayload>() == 10);

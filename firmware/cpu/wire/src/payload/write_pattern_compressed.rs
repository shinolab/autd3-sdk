use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternCompressedPayload {
    pub bank: u8,
    pub format: u8,
    pub count: u8,
    pub reserved: u8,
    pub offset: U32,
}

const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, format) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, count) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, offset) == 4);
const _: () = assert!(core::mem::size_of::<WritePatternCompressedPayload>() == 8);

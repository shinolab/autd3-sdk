use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternRawPayload {
    pub bank: u8,
    pub reserved: u8,
    pub index: U16,
}

const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, index) == 2);
const _: () = assert!(core::mem::size_of::<WritePatternRawPayload>() == 4);

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct UpdateBeginPayload {
    pub length: U32,
    pub crc32: U32,
}

const _: () = assert!(core::mem::offset_of!(UpdateBeginPayload, length) == 0);
const _: () = assert!(core::mem::offset_of!(UpdateBeginPayload, crc32) == 4);
const _: () = assert!(core::mem::size_of::<UpdateBeginPayload>() == 8);

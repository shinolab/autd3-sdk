use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct UpdateChunkPayload {
    pub offset: U32,
    pub data_len: U16,
}

const _: () = assert!(core::mem::offset_of!(UpdateChunkPayload, offset) == 0);
const _: () = assert!(core::mem::offset_of!(UpdateChunkPayload, data_len) == 4);
const _: () = assert!(core::mem::size_of::<UpdateChunkPayload>() == 6);

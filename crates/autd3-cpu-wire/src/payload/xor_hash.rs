use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct XorHashPayload {
    pub sleep_ms: U16,
    pub data_len: U16,
}

const _: () = assert!(core::mem::offset_of!(XorHashPayload, sleep_ms) == 0);
const _: () = assert!(core::mem::offset_of!(XorHashPayload, data_len) == 2);
const _: () = assert!(core::mem::size_of::<XorHashPayload>() == 4);

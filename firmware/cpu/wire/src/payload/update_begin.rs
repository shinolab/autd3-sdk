use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_header;
use crate::Error;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct UpdateBeginPayload {
    pub length: U32,
    pub crc32: U32,
}

impl UpdateBeginPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        read_header(payload).map(|(p, _)| p)
    }
}

const _: () = assert!(core::mem::offset_of!(UpdateBeginPayload, length) == 0);
const _: () = assert!(core::mem::offset_of!(UpdateBeginPayload, crc32) == 4);
const _: () = assert!(core::mem::size_of::<UpdateBeginPayload>() == 8);

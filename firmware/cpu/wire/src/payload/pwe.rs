use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_header;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::layout::PWE_TABLE_SIZE;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct PwePayload {
    pub table: [U16; PWE_TABLE_SIZE],
}

impl PwePayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        read_header(payload).map(|(p, _)| p)
    }
}

const _: () = assert!(core::mem::size_of::<PwePayload>() <= PAYLOAD_BYTES);

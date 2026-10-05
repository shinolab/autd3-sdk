use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::layout::OUTPUT_MASK_WORDS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub words: [U16; OUTPUT_MASK_WORDS],
}

impl OutputMaskPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::PAYLOAD_BYTES;
use crate::layout::OUTPUT_MASK_WORDS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub data: [U16; OUTPUT_MASK_WORDS],
}

const _: () = assert!(core::mem::size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);

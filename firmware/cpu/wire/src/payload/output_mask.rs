use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::PAYLOAD_BYTES;
use crate::params::NUM_TRANSDUCERS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub data: [u8; NUM_TRANSDUCERS],
}

const _: () = assert!(core::mem::size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);

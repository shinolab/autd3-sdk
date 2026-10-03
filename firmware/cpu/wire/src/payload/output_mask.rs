use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_header;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::params::NUM_TRANSDUCERS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub data: [u8; NUM_TRANSDUCERS],
}

impl OutputMaskPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        read_header(payload).map(|(p, _)| p)
    }
}

const _: () = assert!(core::mem::size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);

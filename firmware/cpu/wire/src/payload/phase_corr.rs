use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::fpga_params::NUM_TRANSDUCERS;
use crate::frame::PAYLOAD_BYTES;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct PhaseCorrPayload {
    pub data: [u8; NUM_TRANSDUCERS],
}

impl PhaseCorrPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::size_of::<PhaseCorrPayload>() <= PAYLOAD_BYTES);

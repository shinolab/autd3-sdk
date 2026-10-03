use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_exact;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::layout::GPIO_OUT_NUM;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioOutPayload {
    pub values: [U64; GPIO_OUT_NUM],
}

impl GpioOutPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        read_exact(payload)
    }
}

const _: () = assert!(core::mem::size_of::<GpioOutPayload>() <= PAYLOAD_BYTES);

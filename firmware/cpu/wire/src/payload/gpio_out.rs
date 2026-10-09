use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::layout::GPIO_OUT_NUM;
use crate::value::GpioOut;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioOutPayload {
    pub values: [U64; GPIO_OUT_NUM],
}

impl GpioOutPayload {
    #[must_use]
    pub fn new(outputs: [GpioOut; GPIO_OUT_NUM]) -> Self {
        Self {
            values: outputs.map(|output| U64::new(output.encode())),
        }
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::size_of::<GpioOutPayload>() <= PAYLOAD_BYTES);

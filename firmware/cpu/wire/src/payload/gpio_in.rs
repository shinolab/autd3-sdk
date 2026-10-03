use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::Error;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioInPayload {
    pub gpio_in_0: bool,
    pub gpio_in_1: bool,
    pub gpio_in_2: bool,
    pub gpio_in_3: bool,
}

impl GpioInPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_header(payload).map(|(p, _)| p)
    }
}

const _: () = assert!(core::mem::size_of::<GpioInPayload>() == 4);

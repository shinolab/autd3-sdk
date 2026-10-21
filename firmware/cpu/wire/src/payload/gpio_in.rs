use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
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
        try_read_exact(payload)
    }
}

assert_layout!(GpioInPayload {
    gpio_in_0: 0,
    gpio_in_1: 1,
    gpio_in_2: 2,
    gpio_in_3: 3,
} size 4);

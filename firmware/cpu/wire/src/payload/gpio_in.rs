use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::layout::GPIO_IN_NUM;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioInPayload {
    pub values: [bool; GPIO_IN_NUM],
}

impl GpioInPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

assert_layout!(GpioInPayload { values: 0 } size GPIO_IN_NUM);

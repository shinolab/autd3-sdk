use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioInPayload {
    pub gpio_in_0: u8,
    pub gpio_in_1: u8,
    pub gpio_in_2: u8,
    pub gpio_in_3: u8,
}

const _: () = assert!(core::mem::size_of::<GpioInPayload>() == 4);

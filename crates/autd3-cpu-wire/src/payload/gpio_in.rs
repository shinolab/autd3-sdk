use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioInPayload {
    pub flag: u8,
}

const _: () = assert!(core::mem::size_of::<GpioInPayload>() == 1);

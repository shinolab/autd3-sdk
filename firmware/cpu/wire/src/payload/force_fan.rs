use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ForceFanPayload {
    pub value: u8,
}

const _: () = assert!(core::mem::size_of::<ForceFanPayload>() == 1);

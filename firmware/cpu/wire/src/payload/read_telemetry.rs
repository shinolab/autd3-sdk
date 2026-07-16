use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ReadTelemetryPayload {
    pub counter_id: u8,
}

const _: () = assert!(core::mem::size_of::<ReadTelemetryPayload>() == 1);

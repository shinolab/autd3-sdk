use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigPatternPayload {
    pub bank: u8,
    pub emission_type: u8,
    pub divider: U16,
    pub size: U32,
    pub num_foci: u8,
    pub reserved: u8,
    pub sound_speed: U16,
    pub rep: U16,
}

const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, emission_type) == 1);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, num_foci) == 8);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, sound_speed) == 10);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, rep) == 12);
const _: () = assert!(core::mem::size_of::<ConfigPatternPayload>() == 14);

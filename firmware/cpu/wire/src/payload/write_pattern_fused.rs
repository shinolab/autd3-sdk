use zerocopy::little_endian::{U16, U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternFusedPayload {
    pub bank: u8,
    pub emission_type: u8,
    pub divider: U16,
    pub size: U32,
    pub num_foci: u8,
    pub transition_mode: u8,
    pub sound_speed: U16,
    pub rep: U16,
    pub data_len: U16,
    pub transition_value: U64,
    pub margin_ns: U32,
    pub reserved: U32,
}

const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, emission_type) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, num_foci) == 8);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, transition_mode) == 9);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, sound_speed) == 10);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, rep) == 12);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, data_len) == 14);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, transition_value) == 16);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, margin_ns) == 24);
const _: () = assert!(core::mem::size_of::<WritePatternFusedPayload>() == 32);

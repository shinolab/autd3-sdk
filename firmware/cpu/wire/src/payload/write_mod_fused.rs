use zerocopy::little_endian::{U16, U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModulationFusedPayload {
    pub bank: u8,
    pub transition_mode: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
    pub data_len: U16,
    pub transition_value: U64,
    pub margin_ns: U32,
}

const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, rep) == 8);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, data_len) == 10);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, transition_value) == 12);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, margin_ns) == 20);
const _: () = assert!(core::mem::size_of::<WriteModulationFusedPayload>() == 24);

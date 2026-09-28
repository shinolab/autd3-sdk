use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::PAYLOAD_BYTES;
use crate::params::NUM_TRANSDUCERS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternRawPayload {
    pub bank: u8,
    pub reserved: u8,
    pub index: U16,
    pub phases: [u8; NUM_TRANSDUCERS],
    pub intensities: [u8; NUM_TRANSDUCERS],
}

const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, index) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, phases) == 4);
const _: () =
    assert!(core::mem::offset_of!(WritePatternRawPayload, intensities) == 4 + NUM_TRANSDUCERS);
const _: () = assert!(core::mem::size_of::<WritePatternRawPayload>() <= PAYLOAD_BYTES);

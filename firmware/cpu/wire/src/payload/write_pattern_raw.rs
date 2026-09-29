use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::PAYLOAD_BYTES;
use crate::layout::{PATTERN_RAW_DATA_LEN, PATTERN_RAW_MAX_COUNT};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternRawPayload {
    pub bank: u8,
    pub count: u8,
    pub index: U16,
}

const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, count) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, index) == 2);
const _: () = assert!(core::mem::size_of::<WritePatternRawPayload>() == 4);
const _: () = assert!(
    core::mem::size_of::<WritePatternRawPayload>() + PATTERN_RAW_MAX_COUNT * PATTERN_RAW_DATA_LEN
        <= PAYLOAD_BYTES
);
const _: () = assert!(
    core::mem::size_of::<WritePatternRawPayload>()
        + (PATTERN_RAW_MAX_COUNT + 1) * PATTERN_RAW_DATA_LEN
        > PAYLOAD_BYTES
);

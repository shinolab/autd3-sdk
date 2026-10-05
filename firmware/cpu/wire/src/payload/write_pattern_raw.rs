use zerocopy::little_endian::U16;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::fpga_params::EMISSION_MAX_INDICES;
use crate::layout::{PATTERN_RAW_DATA_LEN, PATTERN_RAW_MAX_COUNT};
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternRawPayload {
    pub bank: PatternBank,
    pub count: u8,
    pub index: U16,
}

impl WritePatternRawPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[[u8; PATTERN_RAW_DATA_LEN]]), Error> {
        let (p, rest) = try_read_header::<Self>(payload)?;
        let count = usize::from(p.count);
        if !(1..=PATTERN_RAW_MAX_COUNT).contains(&count)
            || u32::from(p.index.get()) + count as u32 > EMISSION_MAX_INDICES
        {
            return Err(Error::InvalidPayload);
        }
        let (slots, []) = rest.as_chunks::<PATTERN_RAW_DATA_LEN>() else {
            return Err(Error::InvalidPayload);
        };
        if slots.len() != count {
            return Err(Error::InvalidPayload);
        }
        Ok((p, slots))
    }
}

const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, count) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternRawPayload, index) == 2);
const _: () = assert!(core::mem::size_of::<WritePatternRawPayload>() == 4);
const _: () = assert!(PATTERN_RAW_MAX_COUNT >= 1 && PATTERN_RAW_MAX_COUNT <= u8::MAX as usize);

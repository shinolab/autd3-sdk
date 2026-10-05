use zerocopy::little_endian::U32;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::PAYLOAD_BYTES;
use crate::layout::EMISSION_RAM_WORDS;
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteFociPayload {
    pub bank: PatternBank,
    pub reserved: u8,
    pub offset: U32,
}

impl WriteFociPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = try_read_header::<Self>(payload)?;
        if !data.len().is_multiple_of(2)
            || data.len() > PAYLOAD_BYTES - core::mem::size_of::<Self>()
            || p.offset.get().saturating_add((data.len() / 2) as u32) > EMISSION_RAM_WORDS as u32
        {
            return Err(Error::InvalidPayload);
        }
        Ok((p, data))
    }
}

const _: () = assert!(core::mem::offset_of!(WriteFociPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteFociPayload, offset) == 2);
const _: () = assert!(core::mem::size_of::<WriteFociPayload>() == 6);

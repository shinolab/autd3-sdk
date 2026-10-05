use zerocopy::little_endian::U32;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::PAYLOAD_BYTES;
use crate::layout::MOD_BUFFER_SAMPLES;
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModPayload {
    pub bank: ModulationBank,
    pub reserved: u8,
    pub offset: U32,
}

impl WriteModPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = try_read_header::<Self>(payload)?;
        let offset = p.offset.get();
        if !offset.is_multiple_of(2)
            || data.len() > PAYLOAD_BYTES - core::mem::size_of::<Self>()
            || offset.saturating_add(data.len() as u32) > MOD_BUFFER_SAMPLES as u32
        {
            return Err(Error::InvalidPayload);
        }
        Ok((p, data))
    }
}

const _: () = assert!(core::mem::offset_of!(WriteModPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteModPayload, offset) == 2);
const _: () = assert!(core::mem::size_of::<WriteModPayload>() == 6);

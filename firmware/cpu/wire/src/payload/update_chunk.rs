use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_header;
use crate::Error;
use crate::layout::UPDATE_CHUNK_MAX_DATA_LEN;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct UpdateChunkPayload {
    pub offset: U32,
}

impl UpdateChunkPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = read_header::<Self>(payload)?;
        if data.len() > UPDATE_CHUNK_MAX_DATA_LEN {
            return Err(Error::InvalidPayload);
        }
        Ok((p, data))
    }

    #[must_use]
    pub fn fits_in(&self, data: &[u8], length: u32) -> bool {
        self.offset.get().saturating_add(data.len() as u32) <= length
    }
}

const _: () = assert!(core::mem::offset_of!(UpdateChunkPayload, offset) == 0);
const _: () = assert!(core::mem::size_of::<UpdateChunkPayload>() == 4);

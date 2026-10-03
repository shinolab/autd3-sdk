use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::read_header;
use crate::Error;
use crate::layout::UPDATE_CHUNK_MAX_DATA_LEN;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct UpdateChunkPayload {
    pub offset: U32,
    pub data_len: U16,
}

impl UpdateChunkPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, rest) = read_header::<Self>(payload)?;
        let data_len = usize::from(p.data_len.get());
        if data_len > UPDATE_CHUNK_MAX_DATA_LEN {
            return Err(Error::InvalidPayload);
        }
        let data = rest.get(..data_len).ok_or(Error::InvalidPayload)?;
        Ok((p, data))
    }

    #[must_use]
    pub fn fits_in(&self, length: u32) -> bool {
        self.offset
            .get()
            .saturating_add(u32::from(self.data_len.get()))
            <= length
    }
}

const _: () = assert!(core::mem::offset_of!(UpdateChunkPayload, offset) == 0);
const _: () = assert!(core::mem::offset_of!(UpdateChunkPayload, data_len) == 4);
const _: () = assert!(core::mem::size_of::<UpdateChunkPayload>() == 6);

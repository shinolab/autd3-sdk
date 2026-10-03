use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::layout::{MOD_BUFFER_SAMPLES, MOD_WRITE_MAX_DATA_LEN};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModPayload {
    pub bank: ModulationBank,
    pub reserved: u8,
    pub offset: U32,
    pub data_len: U16,
}

impl WriteModPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, rest) = try_read_header::<Self>(payload)?;
        let offset = p.offset.get();
        let data_len = p.data_len.get();
        if !offset.is_multiple_of(2)
            || usize::from(data_len) > MOD_WRITE_MAX_DATA_LEN
            || offset.saturating_add(u32::from(data_len)) > MOD_BUFFER_SAMPLES as u32
        {
            return Err(Error::InvalidPayload);
        }
        let data = rest
            .get(..usize::from(data_len))
            .ok_or(Error::InvalidPayload)?;
        Ok((p, data))
    }
}

const _: () = assert!(core::mem::offset_of!(WriteModPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteModPayload, offset) == 2);
const _: () = assert!(core::mem::offset_of!(WriteModPayload, data_len) == 6);
const _: () = assert!(core::mem::size_of::<WriteModPayload>() == 8);

use zerocopy::little_endian::U32;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_header};
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
    pub fn new(bank: ModulationBank, offset: usize, len: usize) -> Result<Self, PayloadBuildError> {
        if !offset.is_multiple_of(2) {
            return Err(PayloadBuildError::ModulationOffsetNotEven { offset });
        }
        let end = offset.saturating_add(len);
        if end > MOD_BUFFER_SAMPLES {
            return Err(PayloadBuildError::ModulationWriteExceedsCapacity {
                offset,
                end,
                capacity: MOD_BUFFER_SAMPLES,
            });
        }
        Ok(Self {
            bank,
            reserved: 0,
            offset: U32::new(offset as u32),
        })
    }

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

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    const HEADER: usize = core::mem::size_of::<WriteModPayload>();

    #[rstest]
    #[case(0, 0)]
    #[case(0, 4)]
    #[case(MOD_BUFFER_SAMPLES - 4, 4)]
    #[case(MOD_BUFFER_SAMPLES, 0)]
    fn a_built_header_passes_parse(#[case] offset: usize, #[case] len: usize) {
        let built = WriteModPayload::new(ModulationBank::B1, offset, len).unwrap();
        let mut frame = [0u8; HEADER + 4];
        frame[..HEADER].copy_from_slice(built.as_bytes());
        let (parsed, data) = WriteModPayload::parse(&frame[..HEADER + len]).unwrap();
        assert_eq!(parsed.bank, ModulationBank::B1);
        assert_eq!(parsed.offset.get() as usize, offset);
        assert_eq!(data.len(), len);
    }

    #[rstest]
    #[case(1)]
    #[case(MOD_BUFFER_SAMPLES - 1)]
    fn an_odd_offset_is_rejected(#[case] offset: usize) {
        assert_eq!(
            WriteModPayload::new(ModulationBank::B0, offset, 0).err(),
            Some(PayloadBuildError::ModulationOffsetNotEven { offset })
        );
    }

    #[rstest]
    #[case(MOD_BUFFER_SAMPLES - 2, 3, MOD_BUFFER_SAMPLES + 1)]
    #[case(MOD_BUFFER_SAMPLES + 2, 0, MOD_BUFFER_SAMPLES + 2)]
    #[case(2, usize::MAX, usize::MAX)]
    fn a_write_past_the_buffer_is_rejected(
        #[case] offset: usize,
        #[case] len: usize,
        #[case] end: usize,
    ) {
        assert_eq!(
            WriteModPayload::new(ModulationBank::B0, offset, len).err(),
            Some(PayloadBuildError::ModulationWriteExceedsCapacity {
                offset,
                end,
                capacity: MOD_BUFFER_SAMPLES,
            })
        );
    }
}

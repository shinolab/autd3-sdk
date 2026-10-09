use zerocopy::little_endian::U32;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_header};
use crate::PAYLOAD_BYTES;
use crate::layout::{EMISSION_RAM_WORDS, FOCUS_WORDS, MAX_FOCI_TOTAL};
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteFociPayload {
    pub bank: PatternBank,
    pub reserved: u8,
    pub offset: U32,
}

impl WriteFociPayload {
    pub fn new(
        bank: PatternBank,
        focus_offset: usize,
        focus_count: usize,
    ) -> Result<Self, PayloadBuildError> {
        let end = focus_offset.saturating_add(focus_count);
        if end > MAX_FOCI_TOTAL {
            return Err(PayloadBuildError::FociWriteExceedsCapacity {
                offset: focus_offset,
                end,
                capacity: MAX_FOCI_TOTAL,
            });
        }
        Ok(Self {
            bank,
            reserved: 0,
            offset: U32::new((focus_offset * FOCUS_WORDS) as u32),
        })
    }

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

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    const HEADER: usize = core::mem::size_of::<WriteFociPayload>();
    const FOCUS_BYTES: usize = FOCUS_WORDS * 2;

    #[rstest]
    #[case(0, 0)]
    #[case(0, 2)]
    #[case(3, 1)]
    #[case(MAX_FOCI_TOTAL - 2, 2)]
    #[case(MAX_FOCI_TOTAL, 0)]
    fn a_built_header_passes_parse(#[case] focus_offset: usize, #[case] focus_count: usize) {
        let built = WriteFociPayload::new(PatternBank::B1, focus_offset, focus_count).unwrap();
        let mut frame = [0u8; HEADER + 2 * FOCUS_BYTES];
        frame[..HEADER].copy_from_slice(built.as_bytes());
        let (parsed, data) =
            WriteFociPayload::parse(&frame[..HEADER + focus_count * FOCUS_BYTES]).unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(parsed.offset.get() as usize, focus_offset * FOCUS_WORDS);
        assert_eq!(data.len(), focus_count * FOCUS_BYTES);
    }

    #[rstest]
    #[case(MAX_FOCI_TOTAL - 1, 2, MAX_FOCI_TOTAL + 1)]
    #[case(MAX_FOCI_TOTAL + 1, 0, MAX_FOCI_TOTAL + 1)]
    #[case(1, usize::MAX, usize::MAX)]
    fn a_write_past_the_bank_is_rejected(
        #[case] focus_offset: usize,
        #[case] focus_count: usize,
        #[case] end: usize,
    ) {
        assert_eq!(
            WriteFociPayload::new(PatternBank::B0, focus_offset, focus_count).err(),
            Some(PayloadBuildError::FociWriteExceedsCapacity {
                offset: focus_offset,
                end,
                capacity: MAX_FOCI_TOTAL,
            })
        );
    }
}

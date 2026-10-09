use zerocopy::little_endian::U16;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_header};
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
    pub fn new(bank: PatternBank, count: usize, index: usize) -> Result<Self, PayloadBuildError> {
        if count == 0 {
            return Err(PayloadBuildError::PatternSizeTooSmall {
                size: count,
                min: 1,
            });
        }
        if count > PATTERN_RAW_MAX_COUNT {
            return Err(PayloadBuildError::PatternCountExceedsFrame {
                count,
                max: PATTERN_RAW_MAX_COUNT,
            });
        }
        let last_index = index.saturating_add(count - 1);
        if last_index >= EMISSION_MAX_INDICES as usize {
            return Err(PayloadBuildError::PatternIndexOutOfRange {
                index: last_index,
                max: EMISSION_MAX_INDICES as usize,
            });
        }
        Ok(Self {
            bank,
            count: count as u8,
            index: U16::new(index as u16),
        })
    }

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

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;
    use crate::frame::PAYLOAD_BYTES;

    const HEADER: usize = core::mem::size_of::<WritePatternRawPayload>();
    const MAX_INDICES: usize = EMISSION_MAX_INDICES as usize;

    #[rstest]
    #[case(1, 0)]
    #[case(1, MAX_INDICES - 1)]
    #[case(PATTERN_RAW_MAX_COUNT, MAX_INDICES - PATTERN_RAW_MAX_COUNT)]
    fn a_built_header_passes_parse(#[case] count: usize, #[case] index: usize) {
        let built = WritePatternRawPayload::new(PatternBank::B1, count, index).unwrap();
        let mut frame = [0u8; PAYLOAD_BYTES];
        frame[..HEADER].copy_from_slice(built.as_bytes());
        let (parsed, slots) =
            WritePatternRawPayload::parse(&frame[..HEADER + count * PATTERN_RAW_DATA_LEN]).unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(usize::from(parsed.count), count);
        assert_eq!(usize::from(parsed.index.get()), index);
        assert_eq!(slots.len(), count);
    }

    #[test]
    fn an_empty_write_is_rejected() {
        assert_eq!(
            WritePatternRawPayload::new(PatternBank::B0, 0, 0).err(),
            Some(PayloadBuildError::PatternSizeTooSmall { size: 0, min: 1 })
        );
    }

    #[rstest]
    #[case(PATTERN_RAW_MAX_COUNT + 1)]
    #[case(usize::MAX)]
    fn more_patterns_than_a_frame_carries_are_rejected(#[case] count: usize) {
        assert_eq!(
            WritePatternRawPayload::new(PatternBank::B0, count, 0).err(),
            Some(PayloadBuildError::PatternCountExceedsFrame {
                count,
                max: PATTERN_RAW_MAX_COUNT,
            })
        );
    }

    #[rstest]
    #[case(1, MAX_INDICES, MAX_INDICES)]
    #[case(PATTERN_RAW_MAX_COUNT, MAX_INDICES - PATTERN_RAW_MAX_COUNT + 1, MAX_INDICES)]
    #[case(1, usize::MAX, usize::MAX)]
    fn a_write_past_the_last_index_is_rejected(
        #[case] count: usize,
        #[case] index: usize,
        #[case] last_index: usize,
    ) {
        assert_eq!(
            WritePatternRawPayload::new(PatternBank::B0, count, index).err(),
            Some(PayloadBuildError::PatternIndexOutOfRange {
                index: last_index,
                max: MAX_INDICES,
            })
        );
    }
}

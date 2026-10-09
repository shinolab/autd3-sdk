use zerocopy::little_endian::U16;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_header};
use crate::fpga_params::{EMISSION_MAX_INDICES, NUM_TRANSDUCERS};
use crate::frame::PAYLOAD_BYTES;
use crate::{Error, PatternBank};

crate::wire_enum_u8! {
    #[derive(Default)]
    pub enum PhaseDepth {
        #[default]
        Bits8 = 0x08,
        Bits4 = 0x04,
    }
}

impl PhaseDepth {
    #[must_use]
    pub const fn bytes_per_pattern(self) -> usize {
        match self {
            Self::Bits8 => NUM_TRANSDUCERS,
            Self::Bits4 => NUM_TRANSDUCERS.div_ceil(2),
        }
    }

    #[must_use]
    pub const fn max_count(self) -> usize {
        (PAYLOAD_BYTES - core::mem::size_of::<WritePatternPhasePayload>())
            / self.bytes_per_pattern()
    }

    pub fn pack(self, phases: &[u8], dst: &mut [u8]) {
        match self {
            Self::Bits8 => {
                let (head, tail) = dst.split_at_mut(phases.len());
                head.copy_from_slice(phases);
                tail.fill(0);
            }
            Self::Bits4 => {
                dst.iter_mut().enumerate().for_each(|(i, byte)| {
                    let nibble = |t: usize| phases.get(t).map_or(0, |p| p >> 4);
                    *byte = (nibble(2 * i + 1) << 4) | nibble(2 * i);
                });
            }
        }
    }

    #[must_use]
    pub const fn phase(self, phases: &[u8], t: usize) -> u8 {
        match self {
            Self::Bits8 => phases[t],
            Self::Bits4 => ((phases[t / 2] >> (4 * (t % 2))) & 0x0F) * 0x11,
        }
    }
}

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternPhasePayload {
    pub bank: PatternBank,
    pub depth: PhaseDepth,
    pub count: u8,
    pub intensity: u8,
    pub index: U16,
}

impl WritePatternPhasePayload {
    pub fn new(
        bank: PatternBank,
        depth: PhaseDepth,
        count: usize,
        intensity: u8,
        index: usize,
    ) -> Result<Self, PayloadBuildError> {
        if count == 0 {
            return Err(PayloadBuildError::PatternSizeTooSmall {
                size: count,
                min: 1,
            });
        }
        if count > depth.max_count() {
            return Err(PayloadBuildError::PatternCountExceedsDepth {
                count,
                depth,
                max: depth.max_count(),
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
            depth,
            count: count as u8,
            intensity,
            index: U16::new(index as u16),
        })
    }

    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, rest) = try_read_header::<Self>(payload)?;
        let count = usize::from(p.count);
        if !(1..=p.depth.max_count()).contains(&count)
            || u32::from(p.index.get()) + count as u32 > EMISSION_MAX_INDICES
            || rest.len() != count * p.depth.bytes_per_pattern()
        {
            return Err(Error::InvalidPayload);
        }
        Ok((p, rest))
    }
}

const _: () = assert!(core::mem::offset_of!(WritePatternPhasePayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternPhasePayload, depth) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternPhasePayload, count) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternPhasePayload, intensity) == 3);
const _: () = assert!(core::mem::offset_of!(WritePatternPhasePayload, index) == 4);
const _: () = assert!(core::mem::size_of::<WritePatternPhasePayload>() == 6);
const _: () = assert!(PhaseDepth::Bits8.max_count() <= u8::MAX as usize);
const _: () = assert!(PhaseDepth::Bits4.max_count() <= u8::MAX as usize);

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    const HEADER: usize = core::mem::size_of::<WritePatternPhasePayload>();
    const MAX_INDICES: usize = EMISSION_MAX_INDICES as usize;

    #[rstest]
    #[case(PhaseDepth::Bits8, 1, 0)]
    #[case(PhaseDepth::Bits8, PhaseDepth::Bits8.max_count(), 0)]
    #[case(PhaseDepth::Bits8, 1, MAX_INDICES - 1)]
    #[case(PhaseDepth::Bits4, PhaseDepth::Bits4.max_count(), MAX_INDICES - PhaseDepth::Bits4.max_count())]
    fn a_built_header_passes_parse(
        #[case] depth: PhaseDepth,
        #[case] count: usize,
        #[case] index: usize,
    ) {
        let built =
            WritePatternPhasePayload::new(PatternBank::B1, depth, count, 0x80, index).unwrap();
        let mut frame = [0u8; PAYLOAD_BYTES];
        frame[..HEADER].copy_from_slice(built.as_bytes());
        let (parsed, data) =
            WritePatternPhasePayload::parse(&frame[..HEADER + count * depth.bytes_per_pattern()])
                .unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(parsed.depth, depth);
        assert_eq!(usize::from(parsed.count), count);
        assert_eq!(parsed.intensity, 0x80);
        assert_eq!(usize::from(parsed.index.get()), index);
        assert_eq!(data.len(), count * depth.bytes_per_pattern());
    }

    #[test]
    fn an_empty_write_is_rejected() {
        assert_eq!(
            WritePatternPhasePayload::new(PatternBank::B0, PhaseDepth::Bits8, 0, 0, 0).err(),
            Some(PayloadBuildError::PatternSizeTooSmall { size: 0, min: 1 })
        );
    }

    #[rstest]
    #[case(PhaseDepth::Bits8)]
    #[case(PhaseDepth::Bits4)]
    fn more_patterns_than_the_depth_carries_are_rejected(#[case] depth: PhaseDepth) {
        let count = depth.max_count() + 1;
        assert_eq!(
            WritePatternPhasePayload::new(PatternBank::B0, depth, count, 0, 0).err(),
            Some(PayloadBuildError::PatternCountExceedsDepth {
                count,
                depth,
                max: depth.max_count(),
            })
        );
    }

    #[rstest]
    #[case(1, MAX_INDICES, MAX_INDICES)]
    #[case(2, MAX_INDICES - 1, MAX_INDICES)]
    #[case(1, usize::MAX, usize::MAX)]
    fn a_write_past_the_last_index_is_rejected(
        #[case] count: usize,
        #[case] index: usize,
        #[case] last_index: usize,
    ) {
        assert_eq!(
            WritePatternPhasePayload::new(PatternBank::B0, PhaseDepth::Bits8, count, 0, index)
                .err(),
            Some(PayloadBuildError::PatternIndexOutOfRange {
                index: last_index,
                max: MAX_INDICES,
            })
        );
    }
}

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::layout::{EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, PATTERN_COMPRESSED_MAX_GROUPS};
use crate::params::NUM_TRANSDUCERS;
use crate::{Error, PatternBank};

crate::wire_enum! {
    pub enum PatternFormat {
        PhaseFull = 0x01,
        PhaseHalf = 0x02,
    }
}

impl PatternFormat {
    #[must_use]
    pub const fn patterns_per_word(self) -> u8 {
        match self {
            Self::PhaseFull => 2,
            Self::PhaseHalf => 4,
        }
    }

    #[must_use]
    pub const fn phase(self, word: u16, sub: u8) -> u8 {
        match self {
            Self::PhaseFull => (word >> (8 * sub)) as u8,
            Self::PhaseHalf => {
                let p4 = ((word >> (4 * sub)) & 0x0F) as u8;
                (p4 << 4) | p4
            }
        }
    }
}

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternCompressedPayload {
    pub bank: PatternBank,
    pub format: PatternFormat,
    pub count: u8,
    pub intensity: u8,
    pub offset: U32,
}

impl WritePatternCompressedPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[U16]), Error> {
        let (p, rest) = try_read_header::<Self>(payload)?;
        let per_word = p.format.patterns_per_word();
        let max_count = per_word * PATTERN_COMPRESSED_MAX_GROUPS as u8;
        if !(1..=max_count).contains(&p.count)
            || p.offset.get().saturating_add(
                u32::from(p.count - 1) * EMISSION_SLOT_WORDS as u32 + NUM_TRANSDUCERS as u32,
            ) > EMISSION_RAM_WORDS as u32
        {
            return Err(Error::InvalidPayload);
        }
        let groups = usize::from(p.count.div_ceil(per_word));
        let Ok(words) = <[U16]>::ref_from_bytes_with_elems(rest, groups * NUM_TRANSDUCERS) else {
            return Err(Error::InvalidPayload);
        };
        Ok((p, words))
    }
}

const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, format) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, count) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, intensity) == 3);
const _: () = assert!(core::mem::offset_of!(WritePatternCompressedPayload, offset) == 4);
const _: () = assert!(core::mem::size_of::<WritePatternCompressedPayload>() == 8);

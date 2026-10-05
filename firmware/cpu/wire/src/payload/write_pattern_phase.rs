use zerocopy::little_endian::U16;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
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

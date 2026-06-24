use crate::error::{Error, PayloadError};
use crate::params::{EMISSION_MAX_INDICES, EMISSION_SLOT_WORDS, NUM_TRANSDUCERS};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Emission, PatternBank};

use super::{Distribution, Operation};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, little_endian};

pub const PATTERN_MAX_GAINS_PER_FRAME: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternCompression {
    PhaseFull,
    PhaseHalf,
}

impl PatternCompression {
    #[must_use]
    pub const fn per_frame(self) -> usize {
        match self {
            PatternCompression::PhaseFull => 2,
            PatternCompression::PhaseHalf => 4,
        }
    }

    const fn as_u8(self) -> u8 {
        match self {
            PatternCompression::PhaseFull => 1,
            PatternCompression::PhaseHalf => 2,
        }
    }
}

#[repr(C)]
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
struct CompressedPayload {
    bank: u8,
    format: u8,
    count: u8,
    _reserved: u8,
    offset: little_endian::U32,
    words: [little_endian::U16; NUM_TRANSDUCERS],
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternCompressed<'a> {
    pub bank: PatternBank,
    pub index: u16,
    pub format: PatternCompression,
    pub count: u8,
    pub gains: [&'a [[Emission; NUM_TRANSDUCERS]]; PATTERN_MAX_GAINS_PER_FRAME],
}

impl Operation for WritePatternCompressed<'_> {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(
        &self,
        device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        if device >= self.gains[0].len() {
            return Err(Error::InvalidPayload(
                PayloadError::EmissionsDeviceOutOfRange {
                    device,
                    len: self.gains[0].len(),
                },
            ));
        }
        let last_index = usize::from(self.index) + usize::from(self.count.max(1)) - 1;
        if last_index >= EMISSION_MAX_INDICES {
            return Err(Error::InvalidPayload(
                PayloadError::PatternIndexOutOfRange {
                    index: u16::try_from(last_index).unwrap_or(u16::MAX),
                    max: EMISSION_MAX_INDICES,
                },
            ));
        }
        let offset = u32::try_from(usize::from(self.index) * EMISSION_SLOT_WORDS)
            .expect("bounded by EMISSION_RAM_WORDS");
        let (frame, _) = CompressedPayload::mut_from_prefix(&mut out[..])
            .expect("CompressedPayload fits the payload");
        frame.bank = self.bank.as_u8();
        frame.format = self.format.as_u8();
        frame.count = self.count;
        frame.offset = offset.into();
        for (t, word) in frame.words.iter_mut().enumerate() {
            *word = self.pack_word(device, t).into();
        }
        Ok(Cmd::WritePatternCompressed)
    }
}

impl WritePatternCompressed<'_> {
    fn pack_word(&self, device: usize, t: usize) -> u16 {
        let count = usize::from(self.count.max(1));
        match self.format {
            PatternCompression::PhaseFull => (0..count).fold(0u16, |acc, g| {
                acc | (u16::from(self.gains[g][device][t].phase.0) << (8 * g))
            }),
            PatternCompression::PhaseHalf => (0..count).fold(0u16, |acc, g| {
                acc | (u16::from(self.gains[g][device][t].phase.0 >> 4) << (4 * g))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::WRITE_HEADER_BYTES;
    use crate::value::{Intensity, Phase};

    #[test]
    fn phase_full_packs_two_phases_per_word() {
        let mut g0 = [Emission::default(); NUM_TRANSDUCERS];
        let mut g1 = [Emission::default(); NUM_TRANSDUCERS];
        for (i, (a, b)) in g0.iter_mut().zip(g1.iter_mut()).enumerate() {
            a.phase = Phase(u8::try_from(i % 256).unwrap());
            a.intensity = Intensity(0x12);
            b.phase = Phase(u8::try_from((255 - i % 256) % 256).unwrap());
            b.intensity = Intensity(0x34);
        }
        let p0 = [g0];
        let p1 = [g1];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 4,
            format: PatternCompression::PhaseFull,
            count: 2,
            gains: [&p0, &p1, &[], &[]],
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternCompressed);
        assert_eq!(out[1], 1, "format = PhaseFull");
        assert_eq!(out[2], 2, "count = 2");
        let expected_offset = u32::try_from(4 * EMISSION_SLOT_WORDS).unwrap();
        assert_eq!(&out[4..8], &expected_offset.to_le_bytes());
        for i in 0..NUM_TRANSDUCERS {
            let word = u16::from_le_bytes([
                out[WRITE_HEADER_BYTES + 2 * i],
                out[WRITE_HEADER_BYTES + 2 * i + 1],
            ]);
            let expected = u16::from(g0[i].phase.0) | (u16::from(g1[i].phase.0) << 8);
            assert_eq!(word, expected, "t={i}");
        }
    }

    #[test]
    fn phase_half_packs_four_nibbles_per_word() {
        let mk = |off: u8| {
            let mut g = [Emission::default(); NUM_TRANSDUCERS];
            for (i, e) in g.iter_mut().enumerate() {
                e.phase = Phase(u8::try_from((i + usize::from(off)) % 256).unwrap());
                e.intensity = Intensity(0x55);
            }
            g
        };
        let (g0, g1, g2, g3) = (mk(0), mk(16), mk(32), mk(48));
        let (p0, p1, p2, p3) = ([g0], [g1], [g2], [g3]);
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 8,
            format: PatternCompression::PhaseHalf,
            count: 4,
            gains: [&p0, &p1, &p2, &p3],
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(0, 0, &mut out).unwrap();

        assert_eq!(out[1], 2, "format = PhaseHalf");
        assert_eq!(out[2], 4, "count = 4");
        for i in 0..NUM_TRANSDUCERS {
            let word = u16::from_le_bytes([
                out[WRITE_HEADER_BYTES + 2 * i],
                out[WRITE_HEADER_BYTES + 2 * i + 1],
            ]);
            let expected = u16::from(g0[i].phase.0 >> 4)
                | (u16::from(g1[i].phase.0 >> 4) << 4)
                | (u16::from(g2[i].phase.0 >> 4) << 8)
                | (u16::from(g3[i].phase.0 >> 4) << 12);
            assert_eq!(word, expected, "t={i}");
        }
    }

    #[test]
    fn rejects_last_index_out_of_range() {
        let patterns = [[Emission::default(); NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: u16::try_from(EMISSION_MAX_INDICES - 1).unwrap(),
            format: PatternCompression::PhaseFull,
            count: 2,
            gains: [&patterns, &patterns, &[], &[]],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(0, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn rejects_device_out_of_range() {
        let patterns = [[Emission::default(); NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 0,
            format: PatternCompression::PhaseFull,
            count: 1,
            gains: [&patterns, &[], &[], &[]],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(0, 0, &mut out).is_ok());
        assert!(matches!(
            op.encode(1, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}

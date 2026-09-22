use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::{EMISSION_MAX_INDICES, EMISSION_SLOT_WORDS};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, Phase};

use super::write_pattern_buffer::device_phases;
use super::{Distribution, Operation};
use autd3_cpu_wire::payload::WritePatternCompressedPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::U32;

pub const PATTERN_MAX_PER_FRAME: usize = 4;

const _: () = assert!(
    core::mem::size_of::<WritePatternCompressedPayload>()
        + crate::geometry::Autd3::NUM_TRANSDUCERS * 2
        <= PAYLOAD_BYTES
);

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

    const fn name(self) -> &'static str {
        match self {
            PatternCompression::PhaseFull => "PhaseFull",
            PatternCompression::PhaseHalf => "PhaseHalf",
        }
    }

    const fn as_u8(self) -> u8 {
        match self {
            PatternCompression::PhaseFull => 1,
            PatternCompression::PhaseHalf => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternCompressed<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub format: PatternCompression,
    pub intensity: Intensity,
    pub patterns: [Option<&'a [Vec<Phase>]>; PATTERN_MAX_PER_FRAME],
}

impl crate::sealed::Sealed for WritePatternCompressed<'_> {}

impl Operation for WritePatternCompressed<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let count = self.count();
        if count == 0 {
            return Err(PayloadError::PatternSizeTooSmall {
                size: count,
                min: 1,
            }
            .into());
        }
        if count > self.format.per_frame() {
            return Err(PayloadError::PatternCountExceedsFormat {
                count,
                format: self.format.name(),
                max: self.format.per_frame(),
            }
            .into());
        }
        let last_index = self.index + count - 1;
        if last_index >= EMISSION_MAX_INDICES {
            return Err(PayloadError::PatternIndexOutOfRange {
                index: last_index,
                max: EMISSION_MAX_INDICES,
            }
            .into());
        }
        for pattern in self.patterns.iter().flatten() {
            device_phases(pattern, device)?;
        }
        let offset =
            u32::try_from(self.index * EMISSION_SLOT_WORDS).expect("bounded by EMISSION_RAM_WORDS");
        let (h, rest) = WritePatternCompressedPayload::mut_from_prefix(&mut out[..]).unwrap();
        *h = WritePatternCompressedPayload {
            bank: self.bank.as_u8(),
            format: self.format.as_u8(),
            count: u8::try_from(count).expect("count <= PATTERN_MAX_PER_FRAME"),
            intensity: self.intensity.0,
            offset: U32::new(offset),
        };
        rest.as_chunks_mut::<2>()
            .0
            .iter_mut()
            .take(device.num_transducers())
            .enumerate()
            .for_each(|(t, dst)| {
                *dst = self.pack_word(device.idx(), t).to_le_bytes();
            });
        Ok(Cmd::WritePatternCompressed)
    }
}

impl WritePatternCompressed<'_> {
    fn count(&self) -> usize {
        self.patterns
            .iter()
            .position(Option::is_none)
            .unwrap_or(PATTERN_MAX_PER_FRAME)
    }

    fn pack_word(&self, device: usize, t: usize) -> u16 {
        let (shift, hi) = match self.format {
            PatternCompression::PhaseFull => (8usize, 0u8),
            PatternCompression::PhaseHalf => (4usize, 4u8),
        };
        self.patterns
            .iter()
            .enumerate()
            .filter_map(|(g, &p)| p.map(|s| (g, s[device][t].0)))
            .fold(0u16, |acc, (g, phase)| {
                acc | (u16::from(phase >> hi) << (shift * g))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::test_utils::test_device;
    const HEADER_BYTES: usize = core::mem::size_of::<WritePatternCompressedPayload>();

    #[test]
    fn phase_full_packs_two_phases_per_word() {
        let g0: Vec<Phase> = (0..Autd3::NUM_TRANSDUCERS)
            .map(|i| Phase(u8::try_from(i % 256).unwrap()))
            .collect();
        let g1: Vec<Phase> = (0..Autd3::NUM_TRANSDUCERS)
            .map(|i| Phase(u8::try_from((255 - i % 256) % 256).unwrap()))
            .collect();
        let p0 = [g0];
        let p1 = [g1];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 4,
            format: PatternCompression::PhaseFull,
            intensity: Intensity::MAX,
            patterns: [Some(&p0[..]), Some(&p1[..]), None, None],
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternCompressed);
        assert_eq!(out[1], 1, "format = PhaseFull");
        assert_eq!(out[2], 2, "count = 2");
        assert_eq!(out[3], 0xFF, "intensity");
        let expected_offset = u32::try_from(4 * EMISSION_SLOT_WORDS).unwrap();
        assert_eq!(&out[4..8], &expected_offset.to_le_bytes());
        for i in 0..Autd3::NUM_TRANSDUCERS {
            let word =
                u16::from_le_bytes([out[HEADER_BYTES + 2 * i], out[HEADER_BYTES + 2 * i + 1]]);
            let expected = u16::from(p0[0][i].0) | (u16::from(p1[0][i].0) << 8);
            assert_eq!(word, expected, "t={i}");
        }
    }

    #[test]
    fn phase_half_packs_four_nibbles_per_word() {
        let mk = |off: u8| {
            (0..Autd3::NUM_TRANSDUCERS)
                .map(|i| Phase(u8::try_from((i + usize::from(off)) % 256).unwrap()))
                .collect::<Vec<_>>()
        };
        let (g0, g1, g2, g3) = (mk(0), mk(16), mk(32), mk(48));
        let (p0, p1, p2, p3) = ([g0], [g1], [g2], [g3]);
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 8,
            format: PatternCompression::PhaseHalf,
            intensity: Intensity::MAX,
            patterns: [Some(&p0[..]), Some(&p1[..]), Some(&p2[..]), Some(&p3[..])],
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(out[1], 2, "format = PhaseHalf");
        assert_eq!(out[2], 4, "count = 4");
        for i in 0..Autd3::NUM_TRANSDUCERS {
            let word =
                u16::from_le_bytes([out[HEADER_BYTES + 2 * i], out[HEADER_BYTES + 2 * i + 1]]);
            let expected = u16::from(p0[0][i].0 >> 4)
                | (u16::from(p1[0][i].0 >> 4) << 4)
                | (u16::from(p2[0][i].0 >> 4) << 8)
                | (u16::from(p3[0][i].0 >> 4) << 12);
            assert_eq!(word, expected, "t={i}");
        }
    }

    #[test]
    fn intensity_is_carried_in_the_header() {
        let patterns = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 0,
            format: PatternCompression::PhaseHalf,
            intensity: Intensity(0x42),
            patterns: [Some(&patterns[..]), None, None, None],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(&test_device(0), &mut out).unwrap();
        assert_eq!(out[3], 0x42);
    }

    #[test]
    fn rejects_last_index_out_of_range() {
        let patterns = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES - 1,
            format: PatternCompression::PhaseFull,
            intensity: Intensity::MAX,
            patterns: [Some(&patterns[..]), Some(&patterns[..]), None, None],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn rejects_more_patterns_than_the_format_can_pack() {
        let patterns = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 0,
            format: PatternCompression::PhaseFull,
            intensity: Intensity::MAX,
            patterns: [
                Some(&patterns[..]),
                Some(&patterns[..]),
                Some(&patterns[..]),
                None,
            ],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        let err = op.encode(&test_device(0), &mut out).unwrap_err();
        assert!(matches!(err, Error::InvalidPayload(_)), "{err}");
        assert!(err.to_string().contains("PhaseFull"), "{err}");
    }

    #[test]
    fn the_full_count_each_format_advertises_is_still_accepted() {
        let patterns = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let mut out = [0u8; PAYLOAD_BYTES];
        for (format, count) in [
            (PatternCompression::PhaseFull, 2),
            (PatternCompression::PhaseHalf, 4),
        ] {
            let mut slots: [Option<&[Vec<Phase>]>; PATTERN_MAX_PER_FRAME] =
                [None; PATTERN_MAX_PER_FRAME];
            for slot in slots.iter_mut().take(count) {
                *slot = Some(&patterns[..]);
            }
            let op = WritePatternCompressed {
                bank: PatternBank::B0,
                index: 0,
                format,
                intensity: Intensity::MAX,
                patterns: slots,
            };
            assert!(op.encode(&test_device(0), &mut out).is_ok(), "{count}");
        }
    }

    #[test]
    fn rejects_device_out_of_range() {
        let patterns = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let op = WritePatternCompressed {
            bank: PatternBank::B0,
            index: 0,
            format: PatternCompression::PhaseFull,
            intensity: Intensity::MAX,
            patterns: [Some(&patterns[..]), None, None, None],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&test_device(0), &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}

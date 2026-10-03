use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::EMISSION_MAX_INDICES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, Phase};

use super::write_pattern_buffer::device_phases;
use super::{Distribution, Encoded, Operation, write_header};
use autd3_cpu_wire::payload::{PhaseDepth as WirePhaseDepth, WritePatternPhasePayload};
use zerocopy::IntoBytes;
use zerocopy::little_endian::U16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PhaseDepth {
    #[default]
    Bits8,
    Bits4,
}

impl PhaseDepth {
    #[must_use]
    pub const fn max_count(self) -> usize {
        self.as_wire().max_count()
    }

    const fn as_wire(self) -> WirePhaseDepth {
        match self {
            PhaseDepth::Bits8 => WirePhaseDepth::Bits8,
            PhaseDepth::Bits4 => WirePhaseDepth::Bits4,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternPhase<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub depth: PhaseDepth,
    pub intensity: Intensity,
    pub patterns: &'a [Vec<Vec<Phase>>],
}

impl crate::sealed::Sealed for WritePatternPhase<'_> {}

impl Operation for WritePatternPhase<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let count = self.patterns.len();
        if count == 0 {
            return Err(PayloadError::PatternSizeTooSmall {
                size: count,
                min: 1,
            }
            .into());
        }
        if count > self.depth.max_count() {
            return Err(PayloadError::PatternCountExceedsDepth {
                count,
                depth: self.depth,
                max: self.depth.max_count(),
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
        let wire = self.depth.as_wire();
        let rest = write_header(
            out,
            &WritePatternPhasePayload {
                bank: self.bank,
                depth: wire,
                count: u8::try_from(count).expect("bounded by PhaseDepth::max_count"),
                intensity: self.intensity.0,
                index: U16::new(
                    u16::try_from(self.index).expect("bounded by EMISSION_MAX_INDICES"),
                ),
            },
        );
        for (pattern, dst) in self
            .patterns
            .iter()
            .zip(rest.chunks_mut(wire.bytes_per_pattern()))
        {
            let phases = device_phases(pattern, device)?.as_bytes();
            match self.depth {
                PhaseDepth::Bits8 => {
                    let (head, tail) = dst.split_at_mut(phases.len());
                    head.copy_from_slice(phases);
                    tail.fill(0);
                }
                PhaseDepth::Bits4 => {
                    dst.iter_mut().enumerate().for_each(|(i, byte)| {
                        let nibble = |t: usize| phases.get(t).map_or(0, |p| p >> 4);
                        *byte = nibble(2 * i) | (nibble(2 * i + 1) << 4);
                    });
                }
            }
        }
        Ok(Encoded::header_with_data::<WritePatternPhasePayload>(
            Cmd::WritePatternPhase,
            count * wire.bytes_per_pattern(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::test_utils::test_device;

    fn ramp(offset: usize) -> Vec<Vec<Phase>> {
        vec![
            (0..Autd3::NUM_TRANSDUCERS)
                .map(|t| Phase(u8::try_from((t + offset) % 256).unwrap()))
                .collect(),
        ]
    }

    #[test]
    fn bits8_lays_out_each_pattern_contiguously() {
        let patterns: Vec<_> = (0..PhaseDepth::Bits8.max_count())
            .map(|g| ramp(37 * g))
            .collect();
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            index: 4,
            depth: PhaseDepth::Bits8,
            intensity: Intensity(0x80),
            patterns: &patterns,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(
            cmd,
            Encoded::header_with_data::<WritePatternPhasePayload>(
                Cmd::WritePatternPhase,
                5 * Autd3::NUM_TRANSDUCERS
            )
        );
        assert_eq!(out[1], 8, "depth");
        assert_eq!(out[2], 5, "count");
        assert_eq!(out[3], 0x80, "intensity");
        assert_eq!(&out[4..6], &4u16.to_le_bytes(), "index");
        let data = &out[size_of::<WritePatternPhasePayload>()..];
        for (g, pattern) in patterns.iter().enumerate() {
            assert_eq!(
                &data[g * Autd3::NUM_TRANSDUCERS..][..Autd3::NUM_TRANSDUCERS],
                pattern[0].as_bytes(),
                "pattern {g}"
            );
        }
    }

    #[test]
    fn bits4_packs_two_transducers_per_byte() {
        let patterns: Vec<_> = (0..PhaseDepth::Bits4.max_count())
            .map(|g| ramp(16 * g))
            .collect();
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            index: 8,
            depth: PhaseDepth::Bits4,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        let per_pattern = Autd3::NUM_TRANSDUCERS.div_ceil(2);
        assert_eq!(
            cmd,
            Encoded::header_with_data::<WritePatternPhasePayload>(
                Cmd::WritePatternPhase,
                11 * per_pattern
            )
        );
        assert_eq!(out[1], 4, "depth");
        assert_eq!(out[2], 11, "count");
        let data = &out[size_of::<WritePatternPhasePayload>()..];
        for (g, pattern) in patterns.iter().enumerate() {
            let bytes = &data[g * per_pattern..][..per_pattern];
            for (t, phase) in pattern[0].iter().enumerate() {
                assert_eq!(
                    (bytes[t / 2] >> (4 * (t % 2))) & 0x0F,
                    phase.0 >> 4,
                    "pattern {g} transducer {t}"
                );
            }
            assert_eq!(bytes[per_pattern - 1] >> 4, 0, "unused high nibble");
        }
    }

    #[test]
    fn rejects_an_empty_pattern_list() {
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            index: 0,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &[],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(
                PayloadError::PatternSizeTooSmall { .. }
            ))
        ));
    }

    #[test]
    fn rejects_more_patterns_than_the_depth_carries() {
        for depth in [PhaseDepth::Bits8, PhaseDepth::Bits4] {
            let patterns = vec![ramp(0); depth.max_count() + 1];
            let op = WritePatternPhase {
                bank: PatternBank::B0,
                index: 0,
                depth,
                intensity: Intensity::MAX,
                patterns: &patterns,
            };
            let mut out = [0u8; PAYLOAD_BYTES];
            let err = op.encode(&test_device(0), &mut out).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidPayload(PayloadError::PatternCountExceedsDepth { .. })
                ),
                "{err}"
            );
        }
    }

    #[test]
    fn rejects_last_index_out_of_range() {
        let patterns = vec![ramp(0); 2];
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES - 1,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(
                PayloadError::PatternIndexOutOfRange { .. }
            ))
        ));
    }

    #[test]
    fn rejects_device_out_of_range() {
        let patterns = vec![ramp(0)];
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            index: 0,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&test_device(0), &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}

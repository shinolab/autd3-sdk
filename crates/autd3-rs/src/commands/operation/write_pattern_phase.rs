use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, Phase};

use super::{Distribution, Encoded, Operation, device_slot, write_header};
pub use autd3_cpu_wire::payload::PhaseDepth;
use autd3_cpu_wire::payload::WritePatternPhasePayload;
use zerocopy::IntoBytes;

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
        let header = WritePatternPhasePayload::new(
            self.bank,
            self.depth,
            count,
            self.intensity.0,
            self.index,
        )?;
        let rest = write_header(out, &header);
        for (pattern, dst) in self
            .patterns
            .iter()
            .zip(rest.chunks_mut(self.depth.bytes_per_pattern()))
        {
            let phases = device_slot(pattern, device)?.as_bytes();
            self.depth.pack(phases, dst);
        }
        Ok(Encoded::header_with_data::<WritePatternPhasePayload>(
            Cmd::WritePatternPhase,
            count * self.depth.bytes_per_pattern(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::geometry::Autd3;
    use crate::params::EMISSION_MAX_INDICES;
    use crate::test_utils::{encode, test_device};

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

        let (cmd, out) = encode(&op).unwrap();

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

        let (cmd, out) = encode(&op).unwrap();

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
        assert!(matches!(
            encode(&op),
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
            let err = encode(&op).unwrap_err();
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
        assert!(matches!(
            encode(&op),
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

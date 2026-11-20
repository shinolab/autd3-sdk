use crate::commands::Command;
use crate::datagram::{Datagram, Expansion};
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;
use crate::value::{Intensity, PatternBank, Phase};

use super::{Distribution, Operation, checked_device_row, set_cmd, write_header};
pub use autd3_cpu_wire::payload::PhaseDepth;
use autd3_cpu_wire::payload::{PayloadBuildError, WritePatternPhasePayload};
use zerocopy::IntoBytes;

#[derive(Clone, Debug)]
pub struct WritePatternPhase<'a> {
    pub bank: PatternBank,
    pub start_index: usize,
    pub depth: PhaseDepth,
    pub intensity: Intensity,
    pub patterns: &'a [Vec<Vec<Phase>>],
}

impl Command for WritePatternPhase<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        if self.patterns.is_empty() {
            return Err(PayloadBuildError::PatternSizeTooSmall { size: 0, min: 1 }.into());
        }
        let per_frame = self.depth.max_count();
        for (k, patterns) in self.patterns.chunks(per_frame).enumerate() {
            expansion.push_op(&WritePatternPhaseChunk {
                bank: self.bank,
                start_index: self.start_index.saturating_add(k * per_frame),
                depth: self.depth,
                intensity: self.intensity,
                patterns,
            })?;
        }
        Ok(())
    }
}

struct WritePatternPhaseChunk<'a> {
    bank: PatternBank,
    start_index: usize,
    depth: PhaseDepth,
    intensity: Intensity,
    patterns: &'a [Vec<Vec<Phase>>],
}

impl crate::sealed::Sealed for WritePatternPhaseChunk<'_> {}

impl Operation for WritePatternPhaseChunk<'_> {
    fn waits_for_previous(&self) -> bool {
        false
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let count = self.patterns.len();
        let header = WritePatternPhasePayload::new(
            self.bank,
            self.depth,
            count,
            self.intensity.0,
            self.start_index,
        )?;
        let rest = write_header(out, &header);
        for (pattern, dst) in self
            .patterns
            .iter()
            .zip(rest.chunks_mut(self.depth.bytes_per_pattern()))
        {
            let phases = checked_device_row(pattern, device)?.as_bytes();
            self.depth.pack(phases, dst);
        }
        set_cmd(
            out,
            Cmd::WritePatternPhase,
            size_of::<WritePatternPhasePayload>() + count * self.depth.bytes_per_pattern(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::geometry::Autd3;
    use crate::params::EMISSION_MAX_INDICES;
    use crate::test_utils::{build, cmds, payload};

    fn encode(op: &WritePatternPhase<'_>) -> Result<((Cmd, usize), Vec<u8>), Error> {
        let frames = build(1, op.clone())?;
        assert_eq!(frames.len(), 1);
        let datagram = frames.frame(0).unwrap().datagram_for(0);
        Ok((
            (datagram.cmd, datagram.payload_len),
            datagram.payload.to_vec(),
        ))
    }

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
            start_index: 4,
            depth: PhaseDepth::Bits8,
            intensity: Intensity(0x80),
            patterns: &patterns,
        };

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            (
                Cmd::WritePatternPhase,
                size_of::<WritePatternPhasePayload>() + 5 * Autd3::NUM_TRANSDUCERS
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
            start_index: 8,
            depth: PhaseDepth::Bits4,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };

        let (cmd, out) = encode(&op).unwrap();

        let per_pattern = Autd3::NUM_TRANSDUCERS.div_ceil(2);
        assert_eq!(
            cmd,
            (
                Cmd::WritePatternPhase,
                size_of::<WritePatternPhasePayload>() + 11 * per_pattern
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
            start_index: 0,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &[],
        };
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(PayloadError::Build(
                PayloadBuildError::PatternSizeTooSmall { .. }
            )))
        ));
    }

    #[test]
    fn splits_by_the_depth_from_the_start_index() {
        for depth in [PhaseDepth::Bits8, PhaseDepth::Bits4] {
            let max = depth.max_count();
            let patterns = vec![ramp(0); max + 1];
            let frames = build(
                1,
                WritePatternPhase {
                    bank: PatternBank::B0,
                    start_index: 2,
                    depth,
                    intensity: Intensity::MAX,
                    patterns: &patterns,
                },
            )
            .unwrap();
            assert_eq!(
                cmds(&frames),
                [Cmd::WritePatternPhase, Cmd::WritePatternPhase]
            );
            assert_eq!(
                frames.frame(0).unwrap().distribution(),
                Distribution::PerDevice
            );
            let first = payload(&frames, 0, 0);
            assert_eq!(usize::from(first[2]), max, "count");
            assert_eq!(&first[4..6], &2u16.to_le_bytes(), "index");
            let second = payload(&frames, 1, 0);
            assert_eq!(second[2], 1, "count");
            assert_eq!(
                &second[4..6],
                &u16::try_from(2 + max).unwrap().to_le_bytes(),
                "index"
            );
        }
    }

    #[test]
    fn rejects_last_index_out_of_range() {
        let patterns = vec![ramp(0); 2];
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            start_index: EMISSION_MAX_INDICES - 1,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(PayloadError::Build(
                PayloadBuildError::PatternIndexOutOfRange { .. }
            )))
        ));
    }

    #[test]
    fn rejects_device_out_of_range() {
        let patterns = vec![ramp(0)];
        let op = WritePatternPhase {
            bank: PatternBank::B0,
            start_index: 0,
            depth: PhaseDepth::Bits8,
            intensity: Intensity::MAX,
            patterns: &patterns,
        };
        assert!(build(1, op.clone()).is_ok());
        assert!(matches!(build(2, op), Err(Error::InvalidPayload(_))));
    }
}

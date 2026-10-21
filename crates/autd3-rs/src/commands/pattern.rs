use super::Command;
use crate::commands::operation::{PatternIntensity, WritePatternFused};
use crate::datagram::Expansion;
use crate::error::Error;
use crate::value::{PatternBank, Phase, TransitionMode};

#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    pub bank: PatternBank,
    pub phases: &'a [Vec<Phase>],
    pub intensities: PatternIntensity<'a>,
    pub transition_mode: TransitionMode,
}

impl<'a> Pattern<'a> {
    #[must_use]
    pub fn new(phases: &'a [Vec<Phase>], intensities: impl Into<PatternIntensity<'a>>) -> Self {
        Self::with_bank(PatternBank::B0, phases, intensities)
    }

    #[must_use]
    pub fn with_bank(
        bank: PatternBank,
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
    ) -> Self {
        Self {
            bank,
            phases,
            intensities: intensities.into(),
            transition_mode: TransitionMode::Immediate,
        }
    }
}

impl<'a> Command<'a> for Pattern<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        expansion.push(WritePatternFused {
            bank: self.bank,
            phases: self.phases,
            intensities: self.intensities,
            transition_mode: self.transition_mode,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::operation::WritePatternBuffer;
    use crate::geometry::Autd3;
    use crate::protocol::Cmd;
    use crate::test_utils::{build, cmds, payload};
    use crate::value::Intensity;
    use autd3_cpu_wire::payload::{WritePatternFusedPayload, WritePatternRawPayload};

    #[test]
    fn pattern_expands_to_a_single_fused_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let datagrams = build(2, Pattern::new(&phases, &intensities)).unwrap();

        assert_eq!(cmds(&datagrams), [Cmd::WritePatternFused]);
        for device in 0..2 {
            let fused = payload(&datagrams, 0, device);
            assert_eq!(
                fused.len(),
                size_of::<WritePatternFusedPayload>() + 2 * Autd3::NUM_TRANSDUCERS
            );
            assert_eq!(fused[0], 0, "bank B0");
            assert_eq!(fused[1], 0xFF, "IMMEDIATE");
        }
    }

    #[test]
    fn the_fused_frame_carries_the_same_data_as_the_write_frame() {
        let phases: Vec<Vec<Phase>> = (0..2)
            .map(|d| {
                (0..Autd3::NUM_TRANSDUCERS)
                    .map(|i| Phase(u8::try_from((i + d) % 256).unwrap()))
                    .collect()
            })
            .collect();
        let intensities: Vec<Vec<Intensity>> = (0..2)
            .map(|d| {
                (0..Autd3::NUM_TRANSDUCERS)
                    .map(|i| Intensity(u8::try_from((3 * i + d) % 256).unwrap()))
                    .collect()
            })
            .collect();
        let fused = build(
            2,
            Pattern::with_bank(PatternBank::B1, &phases, &intensities),
        )
        .unwrap();
        let raw = build(
            2,
            WritePatternBuffer::new(PatternBank::B1, 0, &phases, &intensities),
        )
        .unwrap();

        for device in 0..2 {
            assert_eq!(payload(&fused, 0, device)[0], 1, "bank B1");
            assert_eq!(
                &payload(&fused, 0, device)[size_of::<WritePatternFusedPayload>()..],
                &payload(&raw, 0, device)[size_of::<WritePatternRawPayload>()..]
            );
        }
    }

    #[test]
    fn a_non_immediate_transition_reaches_the_fused_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let datagrams = build(
            2,
            Pattern {
                transition_mode: TransitionMode::Ext,
                ..Pattern::new(&phases, &intensities)
            },
        )
        .unwrap();

        assert_eq!(cmds(&datagrams), [Cmd::WritePatternFused]);
        assert_eq!(payload(&datagrams, 0, 0)[1], 0xF0, "EXT");
    }
}

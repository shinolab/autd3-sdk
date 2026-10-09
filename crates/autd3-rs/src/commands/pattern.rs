use super::Command;
use crate::commands::operation::{
    ActivatePatternBank, ConfigPattern, PatternIntensity, WritePatternBuffer,
};
use crate::datagram::Expansion;
use crate::error::Error;
use crate::value::{LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode};
use core::num::NonZeroU16;

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
        expansion
            .push(WritePatternBuffer::new(
                self.bank,
                0,
                self.phases,
                self.intensities,
            ))?
            .push(ConfigPattern {
                bank: self.bank,
                config: SamplingConfig::new(NonZeroU16::MAX),
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            })?;
        if !self.transition_mode.is_later() {
            expansion.push(ActivatePatternBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::protocol::Cmd;
    use crate::test_utils::{build, cmds, payload};
    use crate::value::Intensity;

    #[test]
    fn pattern_expands_to_write_config_activate() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let datagrams = build(2, Pattern::new(&phases, &intensities)).unwrap();

        assert_eq!(
            cmds(&datagrams),
            [
                Cmd::WritePatternRaw,
                Cmd::ConfigPattern,
                Cmd::ActivatePatternBank,
            ],
            "write + config + activate"
        );

        let config = payload(&datagrams, 1, 0);
        assert_eq!(config[0], 0, "bank B0");
        assert_eq!(&config[2..4], &u16::MAX.to_le_bytes());
        assert_eq!(&config[4..8], &1u32.to_le_bytes(), "size = 1 index");
        assert_eq!(&config[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");

        let activate = payload(&datagrams, 2, 0);
        assert_eq!(activate[0], 0, "bank B0");
        assert_eq!(activate[1], 0xFF, "IMMEDIATE");
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let datagrams = build(
            2,
            Pattern {
                transition_mode: TransitionMode::Later,
                ..Pattern::with_bank(PatternBank::B1, &phases, &intensities)
            },
        )
        .unwrap();

        assert_eq!(
            cmds(&datagrams),
            [Cmd::WritePatternRaw, Cmd::ConfigPattern],
            "write + config, no activate"
        );
        let config = payload(&datagrams, 1, 0);
        assert_eq!(config[0], 1, "bank B1");
        assert_eq!(&config[2..4], &u16::MAX.to_le_bytes());
        assert_eq!(&config[4..8], &1u32.to_le_bytes(), "size = 1 index");
        assert_eq!(&config[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
    }

    #[test]
    fn a_non_immediate_transition_reaches_the_activate_frame() {
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

        assert_eq!(datagrams.len(), 3);
        assert_eq!(cmds(&datagrams)[2], Cmd::ActivatePatternBank);
        assert_eq!(payload(&datagrams, 2, 0)[1], 0xF0, "EXT");
    }
}

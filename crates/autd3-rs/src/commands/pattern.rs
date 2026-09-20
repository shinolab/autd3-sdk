use super::Command;
use crate::commands::operation::{
    ConfigPattern, PatternIntensity, WritePatternBuffer, WritePatternFused,
};
use crate::datagram::DatagramBuilder;
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
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        if self.transition_mode.is_later() {
            builder
                .push(WritePatternBuffer::new(
                    self.bank,
                    0,
                    self.phases,
                    self.intensities,
                ))
                .push(ConfigPattern {
                    bank: self.bank,
                    config: SamplingConfig::new(NonZeroU16::MAX),
                    size: 1,
                    loop_behavior: LoopBehavior::Infinite,
                });
            return;
        }
        builder.push(WritePatternFused::new(
            self.bank,
            self.phases,
            self.intensities,
            SamplingConfig::new(NonZeroU16::MAX),
            LoopBehavior::Infinite,
            self.transition_mode,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::mirror::FREQ_DIV_NO_LIMIT;
    use crate::protocol::Cmd;
    use crate::test_utils::test_geometry_arc;
    use crate::value::Intensity;

    #[test]
    fn pattern_expands_to_a_single_fused_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(test_geometry_arc(2));
        b.push(Pattern::new(&phases, &intensities));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 1, "write+config+change fused into 1 frame");

        let f = datagrams.frame(0).unwrap();
        let payload = &f.datagrams()[0].payload;
        assert_eq!(f.datagrams()[0].cmd, Cmd::WritePatternFused);
        assert_eq!(payload[0], 0, "bank B0");
        assert_eq!(&payload[2..4], &FREQ_DIV_NO_LIMIT.to_le_bytes());
        assert_eq!(&payload[4..8], &1u32.to_le_bytes(), "size = 1 index");
        assert_eq!(payload[9], 0xFF, "IMMEDIATE");
        assert_eq!(&payload[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(test_geometry_arc(2));
        b.push(Pattern {
            transition_mode: TransitionMode::Later,
            ..Pattern::with_bank(PatternBank::B1, &phases, &intensities)
        });
        let datagrams = b.build().unwrap();

        assert_eq!(
            datagrams.len(),
            2,
            "write + config, no fusion and no change"
        );
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WritePatternRaw
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        let payload = &cfg.datagrams()[0].payload;
        assert_eq!(payload[0], 1, "bank B1");
        assert_eq!(&payload[2..4], &FREQ_DIV_NO_LIMIT.to_le_bytes());
        assert_eq!(&payload[4..8], &1u32.to_le_bytes(), "size = 1 index");
        assert_eq!(&payload[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
    }

    #[test]
    fn a_non_immediate_transition_reaches_the_fused_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(test_geometry_arc(2));
        b.push(Pattern {
            transition_mode: TransitionMode::Ext,
            ..Pattern::new(&phases, &intensities)
        });
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 1);
        let f = datagrams.frame(0).unwrap();
        assert_eq!(f.datagrams()[0].cmd, Cmd::WritePatternFused);
        assert_eq!(f.datagrams()[0].payload[9], 0xF0, "EXT");
    }
}

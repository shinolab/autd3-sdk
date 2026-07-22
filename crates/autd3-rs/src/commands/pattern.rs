use super::Command;
use crate::commands::operation::WritePatternFused;
use crate::datagram::DatagramBuilder;
use crate::value::{Emission, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};
use core::num::NonZeroU16;

#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    pub bank: PatternBank,
    pub emissions: &'a [Vec<Emission>],
}

impl<'a> Pattern<'a> {
    #[must_use]
    pub fn new(emissions: &'a [Vec<Emission>]) -> Self {
        Self::with_bank(PatternBank::B0, emissions)
    }

    #[must_use]
    pub fn with_bank(bank: PatternBank, emissions: &'a [Vec<Emission>]) -> Self {
        Self { bank, emissions }
    }
}

impl<'a> Command<'a> for Pattern<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        builder.push(WritePatternFused {
            bank: self.bank,
            emissions: self.emissions,
            config: SamplingConfig::new(NonZeroU16::MAX),
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::mirror::FREQ_DIV_NO_LIMIT;
    use crate::protocol::Cmd;
    use crate::test_utils::test_geometry_arc;

    #[test]
    fn pattern_expands_to_a_single_fused_frame() {
        let patterns = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(test_geometry_arc(2));
        b.push(Pattern::new(&patterns));
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
}

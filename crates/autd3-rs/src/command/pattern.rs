use super::Command;
use crate::datagram::DatagramBuilder;
use crate::mirror::FREQ_DIV_NO_LIMIT;
use crate::operation::{ChangePatternBank, ConfigPattern, WritePatternBuffer};
use crate::params::NUM_TRANSDUCERS;
use crate::value::{Emission, LoopBehavior, PatternBank, TransitionMode};

#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    pub bank: PatternBank,
    pub emissions: &'a [[Emission; NUM_TRANSDUCERS]],
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl<'a> Pattern<'a> {
    #[must_use]
    pub fn new(emissions: &'a [[Emission; NUM_TRANSDUCERS]]) -> Self {
        Self::with_bank(PatternBank::B0, emissions)
    }

    #[must_use]
    pub fn with_bank(bank: PatternBank, emissions: &'a [[Emission; NUM_TRANSDUCERS]]) -> Self {
        Self {
            bank,
            emissions,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

impl<'a> Command<'a> for Pattern<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        builder
            .push(WritePatternBuffer {
                bank: self.bank,
                index: 0,
                emissions: self.emissions,
            })
            .push(ConfigPattern {
                bank: self.bank,
                divider: FREQ_DIV_NO_LIMIT,
                size: 1,
                data_type: crate::value::PatternDataType::Raw,
                loop_behavior: self.loop_behavior,
            })
            .push(ChangePatternBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cmd;

    #[test]
    fn pattern_expands_to_write_config_then_change_bank() {
        let patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; 2];
        let mut b = DatagramBuilder::new(2);
        b.push(Pattern::new(&patterns));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3);
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WritePatternBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(
            &cfg.datagrams()[0].payload[2..4],
            &FREQ_DIV_NO_LIMIT.to_le_bytes()
        );

        let chg = datagrams.frame(2).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangePatternBank);
        assert_eq!(chg.datagrams()[0].payload[0], 0, "bank B0");
        assert_eq!(chg.datagrams()[0].payload[1], 0xFF, "IMMEDIATE");
    }
}

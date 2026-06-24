use super::StmConfig;
use crate::command::Command;
use crate::datagram::DatagramBuilder;
use crate::operation::{ChangePatternBank, ConfigPattern, WritePatternBuffer};
use crate::params::NUM_TRANSDUCERS;
use crate::value::{Emission, PatternBank, PatternDataType, TransitionMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GainStmOption {
    pub bank: PatternBank,
}

#[derive(Clone, Copy, Debug)]
pub struct GainStm<'a> {
    pub config: StmConfig,
    pub patterns: &'a [Vec<[Emission; NUM_TRANSDUCERS]>],
    pub option: GainStmOption,
}

impl<'a> GainStm<'a> {
    #[must_use]
    pub fn new(
        config: impl Into<StmConfig>,
        patterns: &'a [Vec<[Emission; NUM_TRANSDUCERS]>],
        option: GainStmOption,
    ) -> Self {
        Self {
            config: config.into(),
            patterns,
            option,
        }
    }
}

impl<'a> Command<'a> for GainStm<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let n = self.patterns.len();
        let divider = self.config.into_sampling_config(n).divide().unwrap_or(0);
        let size = u32::try_from(n).unwrap_or(u32::MAX);
        let bank = self.option.bank;

        for (i, pattern) in self.patterns.iter().enumerate() {
            let index = u16::try_from(i).unwrap_or(u16::MAX);
            builder.push(WritePatternBuffer {
                bank,
                index,
                emissions: pattern.as_slice(),
            });
        }

        builder
            .push(ConfigPattern {
                bank,
                divider,
                size,
                data_type: PatternDataType::Raw,
            })
            .push(ChangePatternBank {
                bank,
                transition_mode: TransitionMode::Immediate,
                transition_value: 0,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::EMISSION_SLOT_WORDS;
    use crate::protocol::Cmd;
    use crate::value::SamplingConfig;

    #[test]
    fn gain_stm_expands_per_index_then_config_change() {
        let patterns: Vec<Vec<[Emission; NUM_TRANSDUCERS]>> = (0..3)
            .map(|_| vec![[Emission::default(); NUM_TRANSDUCERS]])
            .collect();
        let stm = GainStm::new(SamplingConfig::FREQ_4K, &patterns, GainStmOption::default());

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 5);
        for i in 0..3 {
            let f = datagrams.frame(i).unwrap();
            assert_eq!(f.datagrams()[0].cmd, Cmd::WritePatternBuffer);
            let offset = u32::try_from(i * EMISSION_SLOT_WORDS).unwrap();
            assert_eq!(&f.datagrams()[0].payload[2..6], &offset.to_le_bytes());
        }

        let cfg = datagrams.frame(3).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(cfg.datagrams()[0].payload[1], 1, "RawEmissions data_type");
        assert_eq!(
            &cfg.datagrams()[0].payload[2..4],
            &10u16.to_le_bytes(),
            "FREQ_4K divider"
        );
        assert_eq!(
            &cfg.datagrams()[0].payload[4..8],
            &3u32.to_le_bytes(),
            "size = pattern count"
        );

        let chg = datagrams.frame(4).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangePatternBank);
        assert_eq!(chg.datagrams()[0].payload[1], 0xFF, "IMMEDIATE");
    }
}

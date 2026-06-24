use super::StmConfig;
use crate::command::Command;
use crate::datagram::DatagramBuilder;
use crate::operation::{
    ChangePatternBank, ConfigPattern, PATTERN_MAX_GAINS_PER_FRAME, PatternCompression,
    WritePatternBuffer, WritePatternCompressed,
};
use crate::params::NUM_TRANSDUCERS;
use crate::value::{Emission, PatternBank, PatternDataType, TransitionMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GainStmMode {
    #[default]
    PhaseIntensityFull,
    PhaseFull,
    PhaseHalf,
}

impl GainStmMode {
    const fn compression(self) -> Option<PatternCompression> {
        match self {
            GainStmMode::PhaseIntensityFull => None,
            GainStmMode::PhaseFull => Some(PatternCompression::PhaseFull),
            GainStmMode::PhaseHalf => Some(PatternCompression::PhaseHalf),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GainStmOption {
    pub bank: PatternBank,
    pub mode: GainStmMode,
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

        match self.option.mode.compression() {
            None => {
                for (i, pattern) in self.patterns.iter().enumerate() {
                    builder.push(WritePatternBuffer {
                        bank,
                        index: u16::try_from(i).unwrap_or(u16::MAX),
                        emissions: pattern.as_slice(),
                    });
                }
            }
            Some(format) => {
                let per_frame = format.per_frame();
                let mut base = 0;
                while base < n {
                    let count = per_frame.min(n - base);
                    let mut gains: [&'a [[Emission; NUM_TRANSDUCERS]];
                        PATTERN_MAX_GAINS_PER_FRAME] = [&[]; PATTERN_MAX_GAINS_PER_FRAME];
                    for (g, slot) in gains.iter_mut().take(count).enumerate() {
                        *slot = self.patterns[base + g].as_slice();
                    }
                    builder.push(WritePatternCompressed {
                        bank,
                        index: u16::try_from(base).unwrap_or(u16::MAX),
                        format,
                        count: u8::try_from(count).unwrap_or(1),
                        gains,
                    });
                    base += count;
                }
            }
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

    fn make_patterns(n: usize) -> Vec<Vec<[Emission; NUM_TRANSDUCERS]>> {
        use crate::value::{Intensity, Phase};
        (0..n)
            .map(|k| {
                let mut e = [Emission::default(); NUM_TRANSDUCERS];
                for (t, em) in e.iter_mut().enumerate() {
                    em.phase = Phase(u8::try_from((k * 7 + t) % 256).unwrap());
                    em.intensity = Intensity(0x80);
                }
                vec![e]
            })
            .collect()
    }

    #[test]
    fn gain_stm_phase_full_packs_two_indices_per_frame() {
        let patterns = make_patterns(5);
        let stm = GainStm::new(
            SamplingConfig::FREQ_4K,
            &patterns,
            GainStmOption {
                mode: GainStmMode::PhaseFull,
                ..Default::default()
            },
        );

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 5);

        let expected_counts = [2u8, 2, 1];
        let expected_indices = [0u32, 2, 4];
        for (f, (&count, &idx)) in expected_counts
            .iter()
            .zip(expected_indices.iter())
            .enumerate()
        {
            let dg = &datagrams.frame(f).unwrap().datagrams()[0];
            assert_eq!(dg.cmd, Cmd::WritePatternCompressed, "frame {f} cmd");
            let payload = &dg.payload;
            assert_eq!(payload[1], 1, "frame {f} format = PhaseFull");
            assert_eq!(payload[2], count, "frame {f} count");
            let offset = idx * u32::try_from(EMISSION_SLOT_WORDS).unwrap();
            assert_eq!(&payload[4..8], &offset.to_le_bytes(), "frame {f} offset");
            let p0 = patterns[idx as usize][0][0].phase.0;
            assert_eq!(payload[8], p0, "frame {f} low phase");
        }

        let cfg = datagrams.frame(3).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(cfg.datagrams()[0].payload[1], 1, "data_type stays Raw");
        assert_eq!(
            &cfg.datagrams()[0].payload[4..8],
            &5u32.to_le_bytes(),
            "size = total index count"
        );
    }

    #[test]
    fn gain_stm_phase_half_packs_four_indices_per_frame() {
        let patterns = make_patterns(4);
        let stm = GainStm::new(
            SamplingConfig::FREQ_4K,
            &patterns,
            GainStmOption {
                mode: GainStmMode::PhaseHalf,
                ..Default::default()
            },
        );

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3);
        let dg = &datagrams.frame(0).unwrap().datagrams()[0];
        assert_eq!(dg.cmd, Cmd::WritePatternCompressed);
        let payload = &dg.payload;
        assert_eq!(payload[1], 2, "format = PhaseHalf");
        assert_eq!(payload[2], 4, "count = 4");
        let word = u16::from_le_bytes([payload[8], payload[9]]);
        let expected = u16::from(patterns[0][0][0].phase.0 >> 4)
            | (u16::from(patterns[1][0][0].phase.0 >> 4) << 4)
            | (u16::from(patterns[2][0][0].phase.0 >> 4) << 8)
            | (u16::from(patterns[3][0][0].phase.0 >> 4) << 12);
        assert_eq!(word, expected);
    }
}

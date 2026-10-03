use super::StmConfig;
use crate::commands::Command;
use crate::commands::operation::{
    ChangePatternBank, ConfigPattern, PatternIntensity, PhaseDepth, WritePatternBuffers,
    WritePatternPhase,
};
use crate::datagram::DatagramBuilder;
use crate::error::PayloadError;
use crate::params::{BUFFER_SIZE_MIN, EMISSION_MAX_INDICES};
use crate::value::{Intensity, LoopBehavior, PatternBank, Phase, TransitionMode};
use autd3_cpu_wire::layout::PATTERN_RAW_MAX_COUNT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatternStmOption {
    pub bank: PatternBank,
    pub phase_depth: PhaseDepth,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmIntensity<'a> {
    Uniform(Intensity),
    Shared(&'a [Vec<Intensity>]),
    PerIndex(&'a [Vec<Vec<Intensity>>]),
}

impl Default for StmIntensity<'_> {
    fn default() -> Self {
        StmIntensity::Uniform(Intensity::MAX)
    }
}

impl<'a> StmIntensity<'a> {
    #[must_use]
    pub fn at(self, index: usize) -> PatternIntensity<'a> {
        match self {
            StmIntensity::Uniform(intensity) => PatternIntensity::Uniform(intensity),
            StmIntensity::Shared(intensities) => PatternIntensity::PerDevice(intensities),
            StmIntensity::PerIndex(intensities) => PatternIntensity::PerDevice(&intensities[index]),
        }
    }

    const fn per_index_len(self) -> Option<usize> {
        match self {
            StmIntensity::Uniform(_) | StmIntensity::Shared(_) => None,
            StmIntensity::PerIndex(intensities) => Some(intensities.len()),
        }
    }
}

impl From<Intensity> for StmIntensity<'_> {
    fn from(value: Intensity) -> Self {
        StmIntensity::Uniform(value)
    }
}

impl<'a> From<&'a [Vec<Intensity>]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>]) -> Self {
        StmIntensity::Shared(value)
    }
}

impl<'a> From<&'a Vec<Vec<Intensity>>> for StmIntensity<'a> {
    fn from(value: &'a Vec<Vec<Intensity>>) -> Self {
        StmIntensity::Shared(value.as_slice())
    }
}

impl<'a> From<&'a [Vec<Vec<Intensity>>]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Vec<Intensity>>]) -> Self {
        StmIntensity::PerIndex(value)
    }
}

impl<'a> From<&'a Vec<Vec<Vec<Intensity>>>> for StmIntensity<'a> {
    fn from(value: &'a Vec<Vec<Vec<Intensity>>>) -> Self {
        StmIntensity::PerIndex(value.as_slice())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PatternStm<'a> {
    pub config: StmConfig,
    pub phases: &'a [Vec<Vec<Phase>>],
    pub intensities: StmIntensity<'a>,
    pub option: PatternStmOption,
}

impl<'a> PatternStm<'a> {
    #[must_use]
    pub fn new(
        config: impl Into<StmConfig>,
        phases: &'a [Vec<Vec<Phase>>],
        intensities: impl Into<StmIntensity<'a>>,
        option: PatternStmOption,
    ) -> Self {
        Self {
            config: config.into(),
            phases,
            intensities: intensities.into(),
            option,
        }
    }
}

impl<'a> Command<'a> for PatternStm<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let n = self.phases.len();
        if let Some(len) = self.intensities.per_index_len()
            && len != n
        {
            builder.reject(PayloadError::PatternStmLengthMismatch {
                phases: n,
                intensities: len,
            });
            return;
        }
        if n < BUFFER_SIZE_MIN {
            builder.reject(PayloadError::StmSizeOutOfRange {
                size: n,
                min: BUFFER_SIZE_MIN,
                max: EMISSION_MAX_INDICES,
            });
            return;
        }
        let config = self.config.into_sampling_config(n);
        let size = n;
        let bank = self.option.bank;

        match (self.intensities, self.option.phase_depth) {
            (StmIntensity::Uniform(intensity), depth) => {
                let per_frame = depth.max_count();
                for (k, patterns) in self.phases.chunks(per_frame).enumerate() {
                    builder.push(WritePatternPhase {
                        bank,
                        index: k * per_frame,
                        depth,
                        intensity,
                        patterns,
                    });
                }
            }
            (_, PhaseDepth::Bits8) => {
                let mut index = 0;
                while index < n {
                    let count = PATTERN_RAW_MAX_COUNT.min(n - index);
                    let slot = |k: usize| {
                        let i = index + k.min(count - 1);
                        (self.phases[i].as_slice(), self.intensities.at(i))
                    };
                    builder.push(WritePatternBuffers {
                        bank,
                        index,
                        count,
                        slots: core::array::from_fn(slot),
                    });
                    index += count;
                }
            }
            (_, depth) => {
                builder.reject(PayloadError::PhaseDepthRequiresUniformIntensity { depth });
                return;
            }
        }

        builder.push(ConfigPattern {
            bank,
            config,
            size,
            loop_behavior: self.option.loop_behavior,
        });
        if !self.option.transition_mode.is_later() {
            builder.push(ChangePatternBank {
                bank,
                transition_mode: self.option.transition_mode,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Autd3;
    use crate::protocol::Cmd;
    use crate::test_utils::test_geometry_arc;
    use crate::value::SamplingConfig;
    use zerocopy::IntoBytes;

    #[test]
    fn pattern_stm_expands_per_index_then_config_change() {
        let (phases, intensities) = make_patterns(3);
        let stm = PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            &intensities,
            PatternStmOption::default(),
        );

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 4, "two raw frames carry three indices");
        for (frame, (index, count)) in [(0u16, 2u8), (2, 1)].into_iter().enumerate() {
            let f = datagrams.frame(frame).unwrap();
            assert_eq!(f.datagrams()[0].cmd, Cmd::WritePatternRaw);
            assert_eq!(f.datagrams()[0].payload[1], count, "frame {frame} count");
            assert_eq!(&f.datagrams()[0].payload[2..4], &index.to_le_bytes());
        }

        let cfg = datagrams.frame(2).unwrap();
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

        let chg = datagrams.frame(3).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangePatternBank);
        assert_eq!(chg.datagrams()[0].payload[1], 0xFF, "IMMEDIATE");
    }

    #[test]
    fn pattern_stm_uniform_intensity_sends_phase_only() {
        let (phases, _) = make_patterns(7);

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            Intensity(0x80),
            PatternStmOption::default(),
        ));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 4, "5 + 2 indices, config, change");
        let header = size_of::<autd3_cpu_wire::payload::WritePatternPhasePayload>();
        for (frame, (index, count)) in [(0u16, 5u8), (5, 2)].into_iter().enumerate() {
            let dg = &datagrams.frame(frame).unwrap().datagrams()[0];
            assert_eq!(dg.cmd, Cmd::WritePatternPhase, "frame {frame} cmd");
            assert_eq!(dg.payload[1], 8, "frame {frame} depth");
            assert_eq!(dg.payload[2], count, "frame {frame} count");
            assert_eq!(dg.payload[3], 0x80, "frame {frame} intensity");
            assert_eq!(
                &dg.payload[4..6],
                &index.to_le_bytes(),
                "frame {frame} index"
            );
            assert_eq!(
                dg.payload().len(),
                header + usize::from(count) * Autd3::NUM_TRANSDUCERS
            );
            for k in 0..usize::from(count) {
                let at = header + k * Autd3::NUM_TRANSDUCERS;
                assert_eq!(
                    &dg.payload[at..at + Autd3::NUM_TRANSDUCERS],
                    phases[usize::from(index) + k][0].as_bytes(),
                    "frame {frame} pattern {k}"
                );
            }
        }

        let cfg = datagrams.frame(2).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(cfg.datagrams()[0].payload[1], 1, "data_type stays Raw");
        assert_eq!(
            &cfg.datagrams()[0].payload[4..8],
            &7u32.to_le_bytes(),
            "size = total index count"
        );
    }

    #[test]
    fn pattern_stm_shared_intensity_applies_to_every_index() {
        let (phases, _) = make_patterns(3);
        let shared = vec![vec![Intensity(0x42); Autd3::NUM_TRANSDUCERS]];

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            &shared,
            PatternStmOption::default(),
        ));
        let datagrams = b.build().unwrap();

        let header = core::mem::size_of::<autd3_cpu_wire::payload::WritePatternRawPayload>();
        let slot = autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
        for (i, phases) in phases.iter().enumerate() {
            let (frame, k) = (i / PATTERN_RAW_MAX_COUNT, i % PATTERN_RAW_MAX_COUNT);
            let payload = &datagrams.frame(frame).unwrap().datagrams()[0].payload;
            let base = header + k * slot;
            assert_eq!(payload[base], phases[0][0].0, "index {i} phase");
            assert_eq!(
                payload[base + Autd3::NUM_TRANSDUCERS],
                0x42,
                "index {i} intensity"
            );
        }
    }

    #[test]
    fn pattern_stm_rejects_a_single_pattern() {
        use crate::error::Error;

        let (phases, intensities) = make_patterns(1);
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            &intensities,
            PatternStmOption::default(),
        ));

        assert!(matches!(b.build(), Err(Error::InvalidPayload(_))));
    }

    type Patterns = (Vec<Vec<Vec<Phase>>>, Vec<Vec<Vec<Intensity>>>);

    fn make_patterns(n: usize) -> Patterns {
        let phases = (0..n)
            .map(|k| {
                vec![
                    (0..Autd3::NUM_TRANSDUCERS)
                        .map(|t| Phase(u8::try_from((k * 7 + t) % 256).unwrap()))
                        .collect(),
                ]
            })
            .collect();
        let intensities = vec![vec![vec![Intensity(0x80); Autd3::NUM_TRANSDUCERS]]; n];
        (phases, intensities)
    }

    #[test]
    fn pattern_stm_rejects_mismatched_phase_and_intensity_counts() {
        use crate::error::Error;

        let (phases, _) = make_patterns(3);
        let (_, intensities) = make_patterns(2);
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            &intensities,
            PatternStmOption::default(),
        ));

        assert!(matches!(
            b.build(),
            Err(Error::InvalidPayload(
                PayloadError::PatternStmLengthMismatch { .. }
            ))
        ));
    }

    #[test]
    fn pattern_stm_bits4_packs_eleven_indices_per_frame() {
        let (patterns, _) = make_patterns(12);
        let stm = PatternStm::new(
            SamplingConfig::FREQ_4K,
            &patterns,
            Intensity::MAX,
            PatternStmOption {
                phase_depth: PhaseDepth::Bits4,
                ..Default::default()
            },
        );

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 4);
        for (frame, (index, count)) in [(0u16, 11u8), (11, 1)].into_iter().enumerate() {
            let dg = &datagrams.frame(frame).unwrap().datagrams()[0];
            assert_eq!(dg.cmd, Cmd::WritePatternPhase, "frame {frame} cmd");
            assert_eq!(dg.payload[1], 4, "frame {frame} depth");
            assert_eq!(dg.payload[2], count, "frame {frame} count");
            assert_eq!(
                &dg.payload[4..6],
                &index.to_le_bytes(),
                "frame {frame} index"
            );
            let first = &patterns[usize::from(index)][0];
            assert_eq!(
                dg.payload[6],
                (first[0].0 >> 4) | (first[1].0 & 0xF0),
                "frame {frame} first byte"
            );
        }
    }

    #[test]
    fn pattern_stm_non_uniform_intensity_uses_raw_for_bits8_and_rejects_bits4() {
        use crate::error::Error;

        let (patterns, per_index) = make_patterns(4);
        let shared = vec![vec![Intensity(0x80); Autd3::NUM_TRANSDUCERS]];
        for intensities in [
            StmIntensity::Shared(&shared),
            StmIntensity::PerIndex(&per_index),
        ] {
            let mut b = DatagramBuilder::new(test_geometry_arc(1));
            b.push(PatternStm::new(
                SamplingConfig::FREQ_4K,
                &patterns,
                intensities,
                PatternStmOption::default(),
            ));
            let datagrams = b.build().unwrap();
            assert_eq!(
                datagrams.frame(0).unwrap().datagrams()[0].cmd,
                Cmd::WritePatternRaw,
                "{intensities:?}"
            );

            let mut b = DatagramBuilder::new(test_geometry_arc(1));
            b.push(PatternStm::new(
                SamplingConfig::FREQ_4K,
                &patterns,
                intensities,
                PatternStmOption {
                    phase_depth: PhaseDepth::Bits4,
                    ..Default::default()
                },
            ));
            assert!(
                matches!(
                    b.build(),
                    Err(Error::InvalidPayload(
                        PayloadError::PhaseDepthRequiresUniformIntensity {
                            depth: PhaseDepth::Bits4
                        }
                    ))
                ),
                "{intensities:?}"
            );
        }
    }

    #[test]
    fn pattern_stm_loop_behavior_encodes_rep() {
        use crate::value::LoopBehavior;
        use core::num::NonZeroU16;

        let (patterns, intensities) = make_patterns(3);
        let stm = PatternStm::new(
            SamplingConfig::FREQ_4K,
            &patterns,
            &intensities,
            PatternStmOption {
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(5).unwrap()),
                ..Default::default()
            },
        );

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(stm);
        let datagrams = b.build().unwrap();

        let cfg = datagrams.frame(2).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(
            &cfg.datagrams()[0].payload[12..14],
            &4u16.to_le_bytes(),
            "rep = loop_count - 1"
        );
    }

    fn play_on_emulator(
        phases: &[Vec<Vec<Phase>>],
        intensities: StmIntensity<'_>,
        phase_depth: PhaseDepth,
    ) -> autd3_rs_firmware_emulator::Device {
        use crate::protocol::{Seq, TxFrame};

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            phases,
            intensities,
            PatternStmOption {
                bank: PatternBank::B1,
                phase_depth,
                ..Default::default()
            },
        ));
        let datagrams = b.build().unwrap();

        let mut device = autd3_rs_firmware_emulator::Device::new(Autd3::NUM_TRANSDUCERS);
        device.send(&TxFrame::new(Seq::new(0), Cmd::Reset).to_vec());
        for (seq, frame) in datagrams.iter().enumerate() {
            let dg = &frame.datagrams()[0];
            let tx =
                TxFrame::with_payload(Seq::new(u8::try_from(seq).unwrap()), dg.cmd, dg.payload());
            assert_eq!(
                device.send(&tx.to_vec()).status,
                0,
                "frame {seq} {:?}",
                dg.cmd
            );
        }
        device
    }

    #[test]
    fn pattern_stm_emission_ram_matches_on_every_path() {
        let (phases, _) = make_patterns(13);
        let uniform = vec![vec![Intensity(0x80); Autd3::NUM_TRANSDUCERS]];
        for (intensities, depth) in [
            (StmIntensity::Uniform(Intensity(0x80)), PhaseDepth::Bits8),
            (StmIntensity::Shared(&uniform), PhaseDepth::Bits8),
            (StmIntensity::Uniform(Intensity(0x80)), PhaseDepth::Bits4),
        ] {
            let device = play_on_emulator(&phases, intensities, depth);
            for (index, pattern) in phases.iter().enumerate() {
                let expected: Vec<Phase> = pattern[0]
                    .iter()
                    .map(|p| match depth {
                        PhaseDepth::Bits8 => *p,
                        PhaseDepth::Bits4 => Phase((p.0 >> 4) * 0x11),
                    })
                    .collect();
                assert_eq!(
                    device.fpga().emissions_at(1, index),
                    (expected, vec![Intensity(0x80); Autd3::NUM_TRANSDUCERS]),
                    "{intensities:?} {depth:?} index {index}"
                );
            }
        }
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let (patterns, intensities) = make_patterns(3);
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(PatternStm::new(
            SamplingConfig::FREQ_4K,
            &patterns,
            &intensities,
            PatternStmOption {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::Later,
                ..Default::default()
            },
        ));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3, "2 writes + config, no change");
        let cfg = datagrams.frame(2).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(cfg.datagrams()[0].payload[0], 1, "bank B1");
    }
}

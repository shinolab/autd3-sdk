use super::StmConfig;
use crate::commands::Command;
use crate::commands::operation::{
    ActivatePatternBank, ConfigPattern, PhaseDepth, StmIntensity, WritePatternBuffers,
    WritePatternPhase,
};
use crate::datagram::Expansion;
use crate::error::{Error, PayloadError};
use crate::params::{BUFFER_SIZE_MIN, EMISSION_MAX_INDICES};
use crate::value::{LoopBehavior, PatternBank, Phase, TransitionMode};
use autd3_cpu_wire::layout::PATTERN_RAW_MAX_COUNT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatternStmOption {
    pub bank: PatternBank,
    pub phase_depth: PhaseDepth,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
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
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        let n = self.phases.len();
        if let Some(len) = self.intensities.per_index_len()
            && len != n
        {
            return Err(PayloadError::PatternStmLengthMismatch {
                phases: n,
                intensities: len,
            }
            .into());
        }
        if !(BUFFER_SIZE_MIN..=EMISSION_MAX_INDICES).contains(&n) {
            return Err(PayloadError::StmSizeOutOfRange {
                size: n,
                min: BUFFER_SIZE_MIN,
                max: EMISSION_MAX_INDICES,
            }
            .into());
        }
        let config = self.config.into_sampling_config(n);
        let size = n;
        let bank = self.option.bank;

        match (self.intensities, self.option.phase_depth) {
            (StmIntensity::Uniform(intensity), depth) => {
                let per_frame = depth.max_count();
                for (k, patterns) in self.phases.chunks(per_frame).enumerate() {
                    expansion.push(WritePatternPhase {
                        bank,
                        index: k * per_frame,
                        depth,
                        intensity,
                        patterns,
                    })?;
                }
            }
            (_, PhaseDepth::Bits8) => {
                for (k, phases) in self.phases.chunks(PATTERN_RAW_MAX_COUNT).enumerate() {
                    expansion.push(WritePatternBuffers {
                        bank,
                        index: k * PATTERN_RAW_MAX_COUNT,
                        phases,
                        intensities: self.intensities,
                    })?;
                }
            }
            (_, depth) => {
                return Err(PayloadError::PhaseDepthRequiresUniformIntensity { depth }.into());
            }
        }

        expansion.push(ConfigPattern {
            bank,
            config,
            size,
            loop_behavior: self.option.loop_behavior,
        })?;
        if !self.option.transition_mode.is_later() {
            expansion.push(ActivatePatternBank {
                bank,
                transition_mode: self.option.transition_mode,
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
    use crate::value::{Intensity, SamplingConfig};
    use zerocopy::IntoBytes;

    #[test]
    fn pattern_stm_expands_per_index_then_config_activate() {
        let (phases, intensities) = make_patterns(3);
        let stm = PatternStm::new(
            SamplingConfig::FREQ_4K,
            &phases,
            &intensities,
            PatternStmOption::default(),
        );

        let datagrams = build(1, stm).unwrap();

        assert_eq!(
            cmds(&datagrams),
            [
                Cmd::WritePatternRaw,
                Cmd::WritePatternRaw,
                Cmd::ConfigPattern,
                Cmd::ActivatePatternBank,
            ],
            "two raw frames carry three indices"
        );
        for (frame, (index, count)) in [(0u16, 2u8), (2, 1)].into_iter().enumerate() {
            let write = payload(&datagrams, frame, 0);
            assert_eq!(write[1], count, "frame {frame} count");
            assert_eq!(&write[2..4], &index.to_le_bytes());
        }

        let config = payload(&datagrams, 2, 0);
        assert_eq!(config[1], 1, "RawEmissions data_type");
        assert_eq!(&config[2..4], &10u16.to_le_bytes(), "FREQ_4K divider");
        assert_eq!(&config[4..8], &3u32.to_le_bytes(), "size = pattern count");

        assert_eq!(payload(&datagrams, 3, 0)[1], 0xFF, "IMMEDIATE");
    }

    #[test]
    fn pattern_stm_uniform_intensity_sends_phase_only() {
        let (phases, _) = make_patterns(7);

        let datagrams = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                Intensity(0x80),
                PatternStmOption::default(),
            ),
        )
        .unwrap();

        assert_eq!(
            cmds(&datagrams),
            [
                Cmd::WritePatternPhase,
                Cmd::WritePatternPhase,
                Cmd::ConfigPattern,
                Cmd::ActivatePatternBank,
            ],
            "5 + 2 indices, config, activate"
        );
        let header = size_of::<autd3_cpu_wire::payload::WritePatternPhasePayload>();
        for (frame, (index, count)) in [(0u16, 5u8), (5, 2)].into_iter().enumerate() {
            let write = payload(&datagrams, frame, 0);
            assert_eq!(write[1], 8, "frame {frame} depth");
            assert_eq!(write[2], count, "frame {frame} count");
            assert_eq!(write[3], 0x80, "frame {frame} intensity");
            assert_eq!(&write[4..6], &index.to_le_bytes(), "frame {frame} index");
            assert_eq!(
                write.len(),
                header + usize::from(count) * Autd3::NUM_TRANSDUCERS
            );
            for k in 0..usize::from(count) {
                let at = header + k * Autd3::NUM_TRANSDUCERS;
                assert_eq!(
                    &write[at..][..Autd3::NUM_TRANSDUCERS],
                    phases[usize::from(index) + k][0].as_bytes(),
                    "frame {frame} pattern {k}"
                );
            }
        }

        let config = payload(&datagrams, 2, 0);
        assert_eq!(config[1], 1, "data_type stays Raw");
        assert_eq!(
            &config[4..8],
            &7u32.to_le_bytes(),
            "size = total index count"
        );
    }

    #[test]
    fn pattern_stm_shared_intensity_applies_to_every_index() {
        let (phases, _) = make_patterns(3);
        let shared = vec![vec![Intensity(0x42); Autd3::NUM_TRANSDUCERS]];

        let datagrams = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                &shared,
                PatternStmOption::default(),
            ),
        )
        .unwrap();

        let header = core::mem::size_of::<autd3_cpu_wire::payload::WritePatternRawPayload>();
        let slot = autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
        for (i, phases) in phases.iter().enumerate() {
            let (frame, k) = (i / PATTERN_RAW_MAX_COUNT, i % PATTERN_RAW_MAX_COUNT);
            let write = payload(&datagrams, frame, 0);
            let base = header + k * slot;
            assert_eq!(write[base], phases[0][0].0, "index {i} phase");
            assert_eq!(
                write[base + Autd3::NUM_TRANSDUCERS],
                0x42,
                "index {i} intensity"
            );
        }
    }

    #[test]
    fn pattern_stm_rejects_a_single_pattern() {
        let (phases, intensities) = make_patterns(1);
        let result = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                &intensities,
                PatternStmOption::default(),
            ),
        );

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
    }

    #[test]
    fn pattern_stm_rejects_more_patterns_than_the_bank_holds() {
        let (phases, intensities) = make_patterns(EMISSION_MAX_INDICES + 1);
        for stm in [
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                &intensities,
                PatternStmOption::default(),
            ),
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                Intensity::MAX,
                PatternStmOption::default(),
            ),
        ] {
            let result = build(1, stm);
            assert!(matches!(
                result,
                Err(Error::InvalidPayload(
                    PayloadError::StmSizeOutOfRange { .. }
                ))
            ));
        }
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
        let (phases, _) = make_patterns(3);
        let (_, intensities) = make_patterns(2);
        let result = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &phases,
                &intensities,
                PatternStmOption::default(),
            ),
        );

        assert!(matches!(
            result,
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

        let datagrams = build(1, stm).unwrap();

        assert_eq!(datagrams.len(), 4);
        assert_eq!(cmds(&datagrams)[..2], [Cmd::WritePatternPhase; 2]);
        for (frame, (index, count)) in [(0u16, 11u8), (11, 1)].into_iter().enumerate() {
            let write = payload(&datagrams, frame, 0);
            assert_eq!(write[1], 4, "frame {frame} depth");
            assert_eq!(write[2], count, "frame {frame} count");
            assert_eq!(&write[4..6], &index.to_le_bytes(), "frame {frame} index");
            let first = &patterns[usize::from(index)][0];
            assert_eq!(
                write[6],
                (first[0].0 >> 4) | (first[1].0 & 0xF0),
                "frame {frame} first byte"
            );
        }
    }

    #[test]
    fn pattern_stm_non_uniform_intensity_uses_raw_for_bits8_and_rejects_bits4() {
        let (patterns, per_index) = make_patterns(4);
        let shared = vec![vec![Intensity(0x80); Autd3::NUM_TRANSDUCERS]];
        for intensities in [
            StmIntensity::Shared(&shared),
            StmIntensity::PerIndex(&per_index),
        ] {
            let datagrams = build(
                1,
                PatternStm::new(
                    SamplingConfig::FREQ_4K,
                    &patterns,
                    intensities,
                    PatternStmOption::default(),
                ),
            )
            .unwrap();
            assert_eq!(cmds(&datagrams)[0], Cmd::WritePatternRaw, "{intensities:?}");

            let result = build(
                1,
                PatternStm::new(
                    SamplingConfig::FREQ_4K,
                    &patterns,
                    intensities,
                    PatternStmOption {
                        phase_depth: PhaseDepth::Bits4,
                        ..Default::default()
                    },
                ),
            );
            assert!(
                matches!(
                    result,
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

        let datagrams = build(1, stm).unwrap();

        assert_eq!(cmds(&datagrams)[2], Cmd::ConfigPattern);
        assert_eq!(
            &payload(&datagrams, 2, 0)[12..14],
            &4u16.to_le_bytes(),
            "rep = loop_count - 1"
        );
    }

    fn play_on_emulator(
        phases: &[Vec<Vec<Phase>>],
        intensities: StmIntensity<'_>,
        phase_depth: PhaseDepth,
    ) -> autd3_rs_firmware_emulator::Device {
        let datagrams = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                phases,
                intensities,
                PatternStmOption {
                    bank: PatternBank::B1,
                    phase_depth,
                    ..Default::default()
                },
            ),
        )
        .unwrap();

        let mut device = autd3_rs_firmware_emulator::Device::new(Autd3::NUM_TRANSDUCERS);
        device.send(&[0, Cmd::Reset.as_u8()]);
        for (seq, cmd) in cmds(&datagrams).into_iter().enumerate() {
            let tx = [
                &[u8::try_from(seq).unwrap(), cmd.as_u8()],
                payload(&datagrams, seq, 0),
            ]
            .concat();
            assert_eq!(
                device.send(&tx).status,
                crate::protocol::DeviceErrorCode::None,
                "frame {seq} {cmd:?}"
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
                        _ => Phase((p.0 >> 4) * 0x11),
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
        let datagrams = build(
            1,
            PatternStm::new(
                SamplingConfig::FREQ_4K,
                &patterns,
                &intensities,
                PatternStmOption {
                    bank: PatternBank::B1,
                    transition_mode: TransitionMode::Later,
                    ..Default::default()
                },
            ),
        )
        .unwrap();

        assert_eq!(datagrams.len(), 3, "2 writes + config, no activate");
        assert_eq!(cmds(&datagrams)[2], Cmd::ConfigPattern);
        assert_eq!(payload(&datagrams, 2, 0)[0], 1, "bank B1");
    }
}

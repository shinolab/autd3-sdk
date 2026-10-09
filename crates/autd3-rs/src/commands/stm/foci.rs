use super::StmConfig;
use crate::Velocity;
use crate::commands::operation::{ActivatePatternBank, ConfigFociStm};
use crate::commands::{Command, WriteFociBuffer};
use crate::datagram::Expansion;
use crate::error::Error;
use crate::value::{ControlPoints, LoopBehavior, PatternBank, TransitionMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FociStmOption {
    pub bank: PatternBank,
    pub sound_speed: Velocity,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl Default for FociStmOption {
    fn default() -> Self {
        Self {
            bank: PatternBank::B0,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FociStm<'a, const N: usize> {
    pub config: StmConfig,
    pub points: &'a [ControlPoints<N>],
    pub option: FociStmOption,
}

impl<'a, const N: usize> FociStm<'a, N> {
    #[must_use]
    pub fn new(
        config: impl Into<StmConfig>,
        points: &'a [ControlPoints<N>],
        option: FociStmOption,
    ) -> Self {
        Self {
            config: config.into(),
            points,
            option,
        }
    }
}

impl<'a, const N: usize> Command<'a> for FociStm<'a, N> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        let n = self.points.len();
        let config = self.config.into_sampling_config(n);
        let size = n;
        let num_foci = u8::try_from(N).unwrap_or(u8::MAX);

        let bank = self.option.bank;
        expansion
            .push(WriteFociBuffer {
                bank,
                index_offset: 0,
                points: self.points,
            })?
            .push(ConfigFociStm {
                bank,
                config,
                size,
                num_foci,
                sound_speed: self.option.sound_speed,
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
    use crate::geometry::Point3;
    use crate::params::FOCUS_WORDS;
    use crate::protocol::{Cmd, PAYLOAD_BYTES};
    use crate::test_utils::{build, cmds, payload};
    use crate::value::{Intensity, Phase, SamplingConfig};
    use core::num::NonZeroU16;

    use crate::value::{ControlPoint, Focus};
    use autd3_cpu_wire::payload::WriteFociPayload;

    #[test]
    fn foci_stm_expands_to_write_config_activate() {
        let points = [
            ControlPoints::new(
                [ControlPoint::new(Point3::new(0.0, 0.0, 150.0), Phase::ZERO)],
                Intensity(0xAA),
            ),
            ControlPoints::new(
                [ControlPoint::new(Point3::new(0.0, 0.0, 200.0), Phase::ZERO)],
                Intensity(0xBB),
            ),
        ];
        let frames = build(
            1,
            FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default()),
        )
        .unwrap();

        assert_eq!(
            cmds(&frames),
            [
                Cmd::WriteFociBuffer,
                Cmd::ConfigPattern,
                Cmd::ActivatePatternBank,
            ],
            "write + config + activate"
        );
        let write = payload(&frames, 0, 0);
        let config = payload(&frames, 1, 0);
        let activate = payload(&frames, 2, 0);

        assert_eq!(config[1], 0, "Foci emission_type");
        assert_eq!(&config[2..4], &10u16.to_le_bytes(), "FREQ_4K divider");
        assert_eq!(&config[4..8], &2u32.to_le_bytes(), "size = sample count");
        assert_eq!(config[8], 1, "num_foci = N");
        assert_eq!(&config[10..12], &21760u16.to_le_bytes(), "340 m/s * 64");
        assert_eq!(activate[1], 0xFF, "IMMEDIATE");

        let expected = Focus {
            x: 0,
            y: 0,
            z: 6000,
            intensity_or_offset: 0xAA,
        }
        .encode()
        .unwrap();
        let data = &write[size_of::<WriteFociPayload>()..];
        let first = u64::from_le_bytes(data[..8].try_into().unwrap());
        assert_eq!(first, expected);
    }

    #[test]
    fn foci_stm_first_focus_carries_intensity_rest_phase_offset() {
        let points = [
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(1.0, 0.0, 0.0), Phase(0x11)),
                    ControlPoint::new(Point3::new(-1.0, 0.0, 0.0), Phase(0x22)),
                ],
                Intensity(0x80),
            ),
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(2.0, 0.0, 0.0), Phase(0x33)),
                    ControlPoint::new(Point3::new(-2.0, 0.0, 0.0), Phase(0x44)),
                ],
                Intensity(0x40),
            ),
        ];
        let frames = build(
            1,
            FociStm::new(
                SamplingConfig::new(NonZeroU16::MIN),
                &points,
                FociStmOption::default(),
            ),
        )
        .unwrap();

        let data = &payload(&frames, 0, 0)[size_of::<WriteFociPayload>()..];
        let f0 = u64::from_le_bytes(data[..8].try_into().unwrap());
        let f1 = u64::from_le_bytes(data[8..16].try_into().unwrap());
        assert_eq!((f0 >> 54) & 0xFF, 0x80, "first focus = intensity");
        assert_eq!(
            (f1 >> 54) & 0xFF,
            0x11,
            "second focus = phase offset relative to the first"
        );

        assert_eq!(f0 & 0x3_FFFF, 40);
        assert_eq!(f1 & 0x3_FFFF, 0x3_FFD8, "-40 in 18-bit two's complement");

        assert_eq!(payload(&frames, 1, 0)[8], 2, "num_foci = 2");
    }

    #[test]
    fn foci_stm_auto_splits_write_frames() {
        let max_foci_per_frame =
            (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2);
        let points: Vec<ControlPoints<1>> = (0..max_foci_per_frame + 5)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32 * 0.1)))
            .collect();
        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());

        let datagrams = build(1, stm).unwrap();

        assert_eq!(
            cmds(&datagrams),
            [
                Cmd::WriteFociBuffer,
                Cmd::WriteFociBuffer,
                Cmd::ConfigPattern,
                Cmd::ActivatePatternBank,
            ]
        );
        let size = u32::try_from(max_foci_per_frame + 5).unwrap();
        assert_eq!(&payload(&datagrams, 2, 0)[4..8], &size.to_le_bytes());
    }

    #[test]
    fn foci_stm_bank_comes_from_option() {
        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let frames = build(
            1,
            FociStm::new(
                SamplingConfig::FREQ_4K,
                &points,
                FociStmOption {
                    bank: PatternBank::B1,
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        assert_eq!(frames.len(), 3, "write + config + activate");
        for frame in 0..3 {
            assert_eq!(payload(&frames, frame, 0)[0], 1, "frame {frame} bank B1");
        }
    }

    #[test]
    fn foci_stm_loop_behavior_encodes_rep() {
        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];

        let frames = build(
            1,
            FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default()),
        )
        .unwrap();
        assert_eq!(
            &payload(&frames, 1, 0)[12..14],
            &0xFFFFu16.to_le_bytes(),
            "default = infinite"
        );

        let frames = build(
            1,
            FociStm::new(
                SamplingConfig::FREQ_4K,
                &points,
                FociStmOption {
                    loop_behavior: LoopBehavior::ONCE,
                    transition_mode: TransitionMode::SyncIdx,
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        assert_eq!(
            &payload(&frames, 1, 0)[12..14],
            &0u16.to_le_bytes(),
            "ONCE = rep 0"
        );
    }

    #[test]
    fn foci_stm_transition_mode_encodes_into_the_activate_frame() {
        use crate::value::GpioIn;

        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let frames = build(
            1,
            FociStm::new(
                SamplingConfig::FREQ_4K,
                &points,
                FociStmOption {
                    transition_mode: TransitionMode::Gpio(GpioIn::I1),
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        let activate = payload(&frames, 2, 0);

        assert_eq!(activate[1], 0x02, "GPIO");
        assert_eq!(&activate[2..10], &1u64.to_le_bytes());
    }

    #[test]
    fn foci_stm_frequency_is_per_loop_over_all_points() {
        use crate::units::Hz;

        let points: Vec<ControlPoints<1>> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let frames = build(
            1,
            FociStm::new(1000.0 * Hz, &points, FociStmOption::default()),
        )
        .unwrap();
        assert_eq!(&payload(&frames, 1, 0)[2..4], &10u16.to_le_bytes());
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let datagrams = build(
            1,
            FociStm::new(
                SamplingConfig::FREQ_4K,
                &points,
                FociStmOption {
                    bank: PatternBank::B1,
                    transition_mode: TransitionMode::Later,
                    ..Default::default()
                },
            ),
        )
        .unwrap();

        assert_eq!(
            cmds(&datagrams),
            [Cmd::WriteFociBuffer, Cmd::ConfigPattern],
            "write + config, no activate"
        );
        let config = payload(&datagrams, 1, 0);
        assert_eq!(config[0], 1, "bank B1");
        assert_eq!(&config[4..8], &2u32.to_le_bytes());
    }
}

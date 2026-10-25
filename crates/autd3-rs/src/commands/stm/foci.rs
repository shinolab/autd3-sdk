use crate::Velocity;
use crate::commands::Command;
use crate::commands::operation::{ActivatePatternBank, ConfigFociStm, WriteFociBuffer, num_foci};
use crate::datagram::Expansion;
use crate::error::Error;
use crate::value::{ControlPoints, LoopBehavior, PatternBank, SamplingConfig, TransitionMode};

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

#[derive(Clone, Debug)]
pub struct FociStm<'a> {
    pub config: SamplingConfig,
    pub points: &'a [ControlPoints],
    pub option: FociStmOption,
}

impl<'a> FociStm<'a> {
    #[must_use]
    pub const fn new(
        config: SamplingConfig,
        points: &'a [ControlPoints],
        option: FociStmOption,
    ) -> Self {
        Self {
            config,
            points,
            option,
        }
    }
}

impl Command for FociStm<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        let size = self.points.len();
        let num_foci = num_foci(self.points)?;
        let max_len = WriteFociBuffer::max_len(num_foci);

        let bank = self.option.bank;
        for (i, points) in self.points.chunks(max_len).enumerate() {
            expansion.push(WriteFociBuffer {
                bank,
                start_index: i * max_len,
                points,
            })?;
        }
        expansion
            .push(ConfigFociStm {
                bank,
                config: self.config,
                size,
                num_foci,
                sound_speed: self.option.sound_speed,
                loop_behavior: self.option.loop_behavior,
            })?
            .push(ActivatePatternBank {
                bank,
                transition_mode: self.option.transition_mode,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;
    use crate::params::FOCUS_WORDS;
    use crate::protocol::Cmd;
    use crate::test_utils::{build, cmds, payload};
    use crate::value::{Intensity, Phase};
    use core::num::NonZeroU16;

    use crate::error::PayloadError;
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
        let max_foci_per_frame = WriteFociBuffer::max_len(1);
        let points: Vec<ControlPoints> = (0..max_foci_per_frame + 5)
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
    fn foci_stm_splits_on_control_point_boundaries() {
        let max = WriteFociBuffer::max_len(7);
        let points =
            vec![
                ControlPoints::new([ControlPoint::from(Point3::origin()); 7], Intensity::MAX);
                max + 1
            ];
        let frames = build(
            1,
            FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default()),
        )
        .unwrap();

        let first = payload(&frames, 0, 0);
        assert_eq!(
            first.len(),
            size_of::<WriteFociPayload>() + max * 7 * FOCUS_WORDS * 2
        );
        let second = payload(&frames, 1, 0);
        assert_eq!(
            &second[2..6],
            &u32::try_from(max * 7 * FOCUS_WORDS).unwrap().to_le_bytes()
        );
        assert_eq!(
            second.len(),
            size_of::<WriteFociPayload>() + 7 * FOCUS_WORDS * 2
        );
    }

    #[test]
    fn foci_stm_rejects_empty_points() {
        let points: [ControlPoints; 0] = [];
        assert!(matches!(
            build(
                1,
                FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default())
            ),
            Err(Error::InvalidPayload(PayloadError::FociEmpty))
        ));
    }

    #[test]
    fn foci_stm_rejects_a_num_foci_mismatch_across_frames() {
        let max = WriteFociBuffer::max_len(2);
        let pair = ControlPoints::new([ControlPoint::from(Point3::origin()); 2], Intensity::MAX);
        let mut points = vec![pair; max + 1];
        points[max] = ControlPoints::from(Point3::origin());
        assert!(matches!(
            build(
                1,
                FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default())
            ),
            Err(Error::InvalidPayload(PayloadError::NumFociMismatch { index, expected: 2, got: 1 }))
                if index == max
        ));
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
}

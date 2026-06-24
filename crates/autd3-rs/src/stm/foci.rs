#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use super::{ControlPoints, StmConfig};
use crate::Velocity;
use crate::command::Command;
use crate::datagram::DatagramBuilder;
use crate::operation::{ChangePatternBank, ConfigPattern, WriteFociBuffer};
use crate::value::{Focus, LoopBehavior, PatternBank, PatternDataType, TransitionMode};

const FOCUS_UNIT_MM: f32 = 0.025;

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

impl FociStmOption {
    fn sound_speed_value(self) -> u16 {
        (self.sound_speed.m_s() * 64.0).round() as u16
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

fn to_fixed(mm: f32) -> i32 {
    (mm / FOCUS_UNIT_MM).round() as i32
}

impl<'a, const N: usize> Command<'a> for FociStm<'a, N> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let n = self.points.len();
        let divider = self.config.into_sampling_config(n).divide().unwrap_or(0);
        let size = u32::try_from(n).unwrap_or(u32::MAX);
        let num_foci = u8::try_from(N).unwrap_or(u8::MAX);

        let mut foci = Vec::with_capacity(self.points.len() * N);
        for sample in self.points {
            for (j, point) in sample.points.iter().enumerate() {
                let intensity_or_offset = if j == 0 {
                    sample.intensity.0
                } else {
                    point.phase_offset.0
                };
                foci.push(Focus {
                    x: to_fixed(point.point.x),
                    y: to_fixed(point.point.y),
                    z: to_fixed(point.point.z),
                    intensity_or_offset,
                });
            }
        }

        let bank = self.option.bank;
        builder
            .push(WriteFociBuffer {
                bank,
                offset: 0,
                foci,
            })
            .push(ConfigPattern {
                bank,
                divider,
                size,
                data_type: PatternDataType::Foci {
                    num_foci,
                    sound_speed: self.option.sound_speed_value(),
                },
                rep: self.option.loop_behavior.rep(),
            })
            .push(ChangePatternBank {
                bank,
                transition_mode: self.option.transition_mode,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;
    use crate::operation::MAX_FOCI_PER_FRAME;
    use crate::protocol::Cmd;
    use crate::value::{Intensity, Phase, SamplingConfig};
    use core::num::NonZeroU16;

    use super::super::ControlPoint;

    #[test]
    fn foci_stm_expands_to_write_config_change() {
        let points = [ControlPoints::new(
            [ControlPoint::new(Point3::new(0.0, 0.0, 150.0), Phase::ZERO)],
            Intensity(0xAA),
        )];
        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3);

        let write = datagrams.frame(0).unwrap();
        assert_eq!(write.datagrams()[0].cmd, Cmd::WritePatternBuffer);
        let expected = Focus {
            x: 0,
            y: 0,
            z: 6000,
            intensity_or_offset: 0xAA,
        }
        .encode()
        .unwrap();
        let first = u64::from_le_bytes(write.datagrams()[0].payload[8..16].try_into().unwrap());
        assert_eq!(first, expected);

        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        let payload = &cfg.datagrams()[0].payload;
        assert_eq!(payload[1], 0, "Foci data_type");
        assert_eq!(&payload[2..4], &10u16.to_le_bytes(), "FREQ_4K divider");
        assert_eq!(&payload[4..8], &1u32.to_le_bytes(), "size = sample count");
        assert_eq!(payload[8], 1, "num_foci = N");
        assert_eq!(&payload[10..12], &21760u16.to_le_bytes(), "340 m/s * 64");

        let chg = datagrams.frame(2).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangePatternBank);
        assert_eq!(chg.datagrams()[0].payload[1], 0xFF, "IMMEDIATE");
    }

    #[test]
    fn foci_stm_first_focus_carries_intensity_rest_phase_offset() {
        let points = [ControlPoints::new(
            [
                ControlPoint::new(Point3::new(1.0, 0.0, 0.0), Phase(0x11)),
                ControlPoint::new(Point3::new(-1.0, 0.0, 0.0), Phase(0x22)),
            ],
            Intensity(0x80),
        )];
        let stm = FociStm::new(
            SamplingConfig::Divide(NonZeroU16::MIN),
            &points,
            FociStmOption::default(),
        );

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        let write = datagrams.frame(0).unwrap();
        let f0 = u64::from_le_bytes(write.datagrams()[0].payload[8..16].try_into().unwrap());
        let f1 = u64::from_le_bytes(write.datagrams()[0].payload[16..24].try_into().unwrap());
        assert_eq!((f0 >> 54) & 0xFF, 0x80, "first focus = intensity");
        assert_eq!((f1 >> 54) & 0xFF, 0x22, "second focus = phase offset");

        let neg_40 = (-40i32) as u64;
        assert_eq!(f0 & 0x3_FFFF, 40);
        assert_eq!(f1 & 0x3_FFFF, neg_40 & 0x3_FFFF);

        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].payload[8], 2, "num_foci = 2");
    }

    #[test]
    fn foci_stm_auto_splits_write_frames() {
        let points: Vec<ControlPoints<1>> = (0..MAX_FOCI_PER_FRAME + 5)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32 * 0.1)))
            .collect();
        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 4);
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WritePatternBuffer
        );
        assert_eq!(
            datagrams.frame(1).unwrap().datagrams()[0].cmd,
            Cmd::WritePatternBuffer
        );
        assert_eq!(
            datagrams.frame(2).unwrap().datagrams()[0].cmd,
            Cmd::ConfigPattern
        );
        let size = u32::try_from(MAX_FOCI_PER_FRAME + 5).unwrap();
        assert_eq!(
            &datagrams.frame(2).unwrap().datagrams()[0].payload[4..8],
            &size.to_le_bytes()
        );
    }

    #[test]
    #[allow(clippy::needless_update)]
    fn foci_stm_option_default_via_spread_stays_non_breaking() {
        let option = FociStmOption {
            sound_speed: Velocity::from_m_s(350.0),
            ..Default::default()
        };
        assert_eq!(
            option.sound_speed_value(),
            (350.0_f32 * 64.0).round() as u16
        );
    }

    #[test]
    fn foci_stm_bank_comes_from_option() {
        let points = [ControlPoints::from(Point3::new(0.0, 0.0, 1.0))];
        let option = FociStmOption {
            bank: PatternBank::B1,
            ..Default::default()
        };
        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, option);

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        for i in 0..3 {
            assert_eq!(
                datagrams.frame(i).unwrap().datagrams()[0].payload[0],
                1,
                "bank B1"
            );
        }
    }

    #[test]
    fn foci_stm_loop_behavior_encodes_rep() {
        use crate::value::LoopBehavior;

        let points = [ControlPoints::from(Point3::new(0.0, 0.0, 1.0))];

        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());
        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let cfg = b.build().unwrap();
        assert_eq!(
            &cfg.frame(1).unwrap().datagrams()[0].payload[12..14],
            &0xFFFFu16.to_le_bytes(),
            "default = infinite"
        );

        let stm = FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                loop_behavior: LoopBehavior::ONCE,
                ..Default::default()
            },
        );
        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let cfg = b.build().unwrap();
        assert_eq!(
            &cfg.frame(1).unwrap().datagrams()[0].payload[12..14],
            &0u16.to_le_bytes(),
            "ONCE = rep 0"
        );
    }

    #[test]
    fn foci_stm_transition_mode_encodes_into_change_bank() {
        use crate::value::{GpioIn, TransitionMode};

        let points = [ControlPoints::from(Point3::new(0.0, 0.0, 1.0))];
        let stm = FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                transition_mode: TransitionMode::Gpio(GpioIn::I1),
                ..Default::default()
            },
        );
        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        let chg = datagrams.frame(2).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangePatternBank);
        assert_eq!(chg.datagrams()[0].payload[1], 0x02, "GPIO");
        assert_eq!(&chg.datagrams()[0].payload[2..10], &1u64.to_le_bytes());
    }

    #[test]
    fn foci_stm_frequency_is_per_loop_over_all_points() {
        use crate::units::Hz;

        let points: Vec<ControlPoints<1>> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let stm = FociStm::new(1000.0 * Hz, &points, FociStmOption::default());

        let mut b = DatagramBuilder::new(1);
        b.push(stm);
        let datagrams = b.build().unwrap();

        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(&cfg.datagrams()[0].payload[2..4], &10u16.to_le_bytes());
    }
}

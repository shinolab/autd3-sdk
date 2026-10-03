use super::StmConfig;
use crate::Velocity;
use crate::commands::operation::{ActivatePatternBank, ConfigFociStm};
use crate::commands::{Command, WriteFociBuffer};
use crate::datagram::DatagramBuilder;
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
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let n = self.points.len();
        let config = self.config.into_sampling_config(n);
        let size = n;
        let num_foci = u8::try_from(N).unwrap_or(u8::MAX);

        let bank = self.option.bank;
        builder
            .push(WriteFociBuffer {
                bank,
                index_offset: 0,
                points: self.points,
            })
            .push(ConfigFociStm {
                bank,
                config,
                size,
                num_foci,
                sound_speed: self.option.sound_speed,
                loop_behavior: self.option.loop_behavior,
            });
        if !self.option.transition_mode.is_later() {
            builder.push(ActivatePatternBank {
                bank,
                transition_mode: self.option.transition_mode,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;
    use crate::params::FOCUS_WORDS;
    use crate::protocol::{Cmd, PAYLOAD_BYTES};
    use crate::test_utils::test_geometry_arc;
    use crate::value::{Intensity, Phase, SamplingConfig};
    use core::num::NonZeroU16;

    use crate::value::{ControlPoint, Focus};
    use autd3_cpu_wire::payload::WriteFociPayload;

    fn payloads<const N: usize>(stm: FociStm<'_, N>) -> [[u8; PAYLOAD_BYTES]; 3] {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(stm);
        let datagrams = b.build().unwrap();
        assert_eq!(datagrams.len(), 3, "write + config + change");
        let cmds = [
            Cmd::WriteFociBuffer,
            Cmd::ConfigPattern,
            Cmd::ActivatePatternBank,
        ];
        core::array::from_fn(|i| {
            let f = datagrams.frame(i).unwrap();
            assert_eq!(f.datagrams()[0].cmd, cmds[i]);
            f.datagrams()[0].payload
        })
    }

    #[test]
    fn foci_stm_expands_to_write_config_change() {
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
        let [write, config, change] = payloads(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption::default(),
        ));

        assert_eq!(config[1], 0, "Foci emission_type");
        assert_eq!(&config[2..4], &10u16.to_le_bytes(), "FREQ_4K divider");
        assert_eq!(&config[4..8], &2u32.to_le_bytes(), "size = sample count");
        assert_eq!(config[8], 1, "num_foci = N");
        assert_eq!(&config[10..12], &21760u16.to_le_bytes(), "340 m/s * 64");
        assert_eq!(change[1], 0xFF, "IMMEDIATE");

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
        let [write, config, _] = payloads(FociStm::new(
            SamplingConfig::new(NonZeroU16::MIN),
            &points,
            FociStmOption::default(),
        ));

        let data = &write[size_of::<WriteFociPayload>()..];
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

        assert_eq!(config[8], 2, "num_foci = 2");
    }

    #[test]
    fn foci_stm_auto_splits_write_frames() {
        let max_foci_per_frame =
            (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2);
        let points: Vec<ControlPoints<1>> = (0..max_foci_per_frame + 5)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32 * 0.1)))
            .collect();
        let stm = FociStm::new(SamplingConfig::FREQ_4K, &points, FociStmOption::default());

        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(stm);
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 4);
        assert_eq!(
            datagrams.frame(3).unwrap().datagrams()[0].cmd,
            Cmd::ActivatePatternBank
        );
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteFociBuffer
        );
        assert_eq!(
            datagrams.frame(1).unwrap().datagrams()[0].cmd,
            Cmd::WriteFociBuffer
        );
        assert_eq!(
            datagrams.frame(2).unwrap().datagrams()[0].cmd,
            Cmd::ConfigPattern
        );
        let size = u32::try_from(max_foci_per_frame + 5).unwrap();
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
        assert_eq!(option.sound_speed, Velocity::from_m_s(350.0));
    }

    #[test]
    fn foci_stm_bank_comes_from_option() {
        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let payloads = payloads(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                bank: PatternBank::B1,
                ..Default::default()
            },
        ));
        for payload in payloads {
            assert_eq!(payload[0], 1, "bank B1");
        }
    }

    #[test]
    fn foci_stm_loop_behavior_encodes_rep() {
        use crate::value::LoopBehavior;

        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];

        let [_, config, _] = payloads(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption::default(),
        ));
        assert_eq!(
            &config[12..14],
            &0xFFFFu16.to_le_bytes(),
            "default = infinite"
        );

        let [_, config, _] = payloads(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                loop_behavior: LoopBehavior::ONCE,
                transition_mode: crate::value::TransitionMode::SyncIdx,
                ..Default::default()
            },
        ));
        assert_eq!(&config[12..14], &0u16.to_le_bytes(), "ONCE = rep 0");
    }

    #[test]
    fn foci_stm_transition_mode_encodes_into_the_change_frame() {
        use crate::value::{GpioIn, TransitionMode};

        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let [_, _, change] = payloads(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                transition_mode: TransitionMode::Gpio(GpioIn::I1),
                ..Default::default()
            },
        ));

        assert_eq!(change[1], 0x02, "GPIO");
        assert_eq!(&change[2..10], &1u64.to_le_bytes());
    }

    #[test]
    fn foci_stm_frequency_is_per_loop_over_all_points() {
        use crate::units::Hz;

        let points: Vec<ControlPoints<1>> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let [_, config, _] = payloads(FociStm::new(1000.0 * Hz, &points, FociStmOption::default()));
        assert_eq!(&config[2..4], &10u16.to_le_bytes());
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 1.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 2.0)),
        ];
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(FociStm::new(
            SamplingConfig::FREQ_4K,
            &points,
            FociStmOption {
                bank: PatternBank::B1,
                transition_mode: TransitionMode::Later,
                ..Default::default()
            },
        ));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 2, "write + config, no change");
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteFociBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigPattern);
        assert_eq!(cfg.datagrams()[0].payload[0], 1, "bank B1");
        assert_eq!(&cfg.datagrams()[0].payload[4..8], &2u32.to_le_bytes());
    }
}

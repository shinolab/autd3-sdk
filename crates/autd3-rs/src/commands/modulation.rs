use super::{Command, WriteModulationBuffer};
use crate::commands::operation::{ActivateModulationBank, ConfigModulation};
use crate::datagram::DatagramBuilder;
use crate::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};

#[derive(Clone, Copy, Debug)]
pub struct Modulation<'a> {
    pub bank: ModulationBank,
    pub config: SamplingConfig,
    pub data: &'a [u8],
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl<'a> Modulation<'a> {
    #[must_use]
    pub fn new(config: SamplingConfig, data: &'a [u8]) -> Self {
        Self::with_bank(ModulationBank::B0, config, data)
    }

    #[must_use]
    pub fn with_bank(bank: ModulationBank, config: SamplingConfig, data: &'a [u8]) -> Self {
        Self {
            bank,
            config,
            data,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

impl<'a> Command<'a> for Modulation<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        builder
            .push(WriteModulationBuffer {
                bank: self.bank,
                offset: 0,
                data: self.data,
            })
            .push(ConfigModulation {
                bank: self.bank,
                config: self.config,
                size: self.data.len(),
                loop_behavior: self.loop_behavior,
            });
        if !self.transition_mode.is_later() {
            builder.push(ActivateModulationBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cmd;
    use crate::test_utils::test_geometry_arc;

    fn config_and_change_payloads(m: Modulation<'_>) -> (Vec<u8>, Vec<u8>) {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(m);
        let datagrams = b.build().unwrap();
        assert_eq!(datagrams.len(), 3, "write + config + change");
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteModulationBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigModulation);
        let change = datagrams.frame(2).unwrap();
        assert_eq!(change.datagrams()[0].cmd, Cmd::ActivateModulationBank);
        (
            cfg.datagrams()[0].payload().to_vec(),
            change.datagrams()[0].payload().to_vec(),
        )
    }

    #[test]
    fn modulation_expands_with_size_from_data() {
        let data = vec![0x80u8; 20];
        let (config, change) = config_and_change_payloads(Modulation::with_bank(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            &data,
        ));

        assert_eq!(config[0], 1, "bank B1");
        assert_eq!(&config[4..8], &20u32.to_le_bytes(), "size");
        assert_eq!(change[0], 1, "bank B1");
    }

    #[test]
    fn modulation_defaults_to_infinite_loop() {
        let data = vec![0x80u8; 4];
        let (config, _) =
            config_and_change_payloads(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(&config[8..10], &0xFFFFu16.to_le_bytes());
    }

    #[test]
    fn modulation_defaults_to_immediate_transition() {
        let data = vec![0x80u8; 4];
        let (_, change) =
            config_and_change_payloads(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(change[1], 0xFF, "IMMEDIATE");
        assert_eq!(&change[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn modulation_transition_mode_encodes_into_the_change_frame() {
        use crate::value::{SysTime, TransitionMode};

        let data = vec![0x80u8; 4];
        let (_, change) = config_and_change_payloads(Modulation {
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0xDEAD_BEEF),
                margin: None,
            },
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });

        assert_eq!(change[1], 0x01, "SYS_TIME");
        assert_eq!(&change[2..10], &0xDEAD_BEEFu64.to_le_bytes());
    }

    #[test]
    fn modulation_finite_loop_encodes_rep() {
        use crate::value::LoopBehavior;
        use core::num::NonZeroU16;

        let data = vec![0x80u8; 4];
        let (config, _) = config_and_change_payloads(Modulation {
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });
        assert_eq!(&config[8..10], &9u16.to_le_bytes());
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let data = vec![0x80u8; 4];
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(Modulation {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Later,
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 2, "write + config, no change");
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteModulationBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigModulation);
        assert_eq!(cfg.datagrams()[0].payload[0], 1, "bank B1");
        assert_eq!(&cfg.datagrams()[0].payload[4..8], &4u32.to_le_bytes());
    }

    #[test]
    fn later_skips_the_loop_behavior_constraint() {
        use crate::value::LoopBehavior;
        use core::num::NonZeroU16;

        let data = vec![0x80u8; 4];
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(Modulation {
            bank: ModulationBank::B1,
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
            transition_mode: TransitionMode::Later,
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });
        assert!(b.build().is_ok(), "no transition, so no constraint applies");
    }
}

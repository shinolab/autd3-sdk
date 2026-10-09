use super::{Command, WriteModulationBuffer};
use crate::commands::operation::{ActivateModulationBank, ConfigModulation};
use crate::datagram::Expansion;
use crate::error::Error;
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
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        expansion
            .push(WriteModulationBuffer {
                bank: self.bank,
                offset: 0,
                data: self.data,
            })?
            .push(ConfigModulation {
                bank: self.bank,
                config: self.config,
                size: self.data.len(),
                loop_behavior: self.loop_behavior,
            })?;
        if !self.transition_mode.is_later() {
            expansion.push(ActivateModulationBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cmd;
    use crate::test_utils::{build, cmds, payload};

    fn config_and_activate_payloads(m: Modulation<'_>) -> (Vec<u8>, Vec<u8>) {
        let datagrams = build(1, m).unwrap();
        assert_eq!(
            cmds(&datagrams),
            [
                Cmd::WriteModulationBuffer,
                Cmd::ConfigModulation,
                Cmd::ActivateModulationBank,
            ],
            "write + config + activate"
        );
        (
            payload(&datagrams, 1, 0).to_vec(),
            payload(&datagrams, 2, 0).to_vec(),
        )
    }

    #[test]
    fn modulation_expands_with_size_from_data() {
        let data = vec![0x80u8; 20];
        let (config, activate) = config_and_activate_payloads(Modulation::with_bank(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            &data,
        ));

        assert_eq!(config[0], 1, "bank B1");
        assert_eq!(&config[4..8], &20u32.to_le_bytes(), "size");
        assert_eq!(activate[0], 1, "bank B1");
    }

    #[test]
    fn modulation_defaults_to_infinite_loop() {
        let data = vec![0x80u8; 4];
        let (config, _) =
            config_and_activate_payloads(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(&config[8..10], &0xFFFFu16.to_le_bytes());
    }

    #[test]
    fn modulation_defaults_to_immediate_transition() {
        let data = vec![0x80u8; 4];
        let (_, activate) =
            config_and_activate_payloads(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(activate[1], 0xFF, "IMMEDIATE");
        assert_eq!(&activate[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn modulation_transition_mode_encodes_into_the_activate_frame() {
        use crate::value::SysTime;

        let data = vec![0x80u8; 4];
        let (_, activate) = config_and_activate_payloads(Modulation {
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0xDEAD_BEEF),
            },
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });

        assert_eq!(activate[1], 0x01, "SYS_TIME");
        assert_eq!(&activate[2..10], &0xDEAD_BEEFu64.to_le_bytes());
    }

    #[test]
    fn modulation_finite_loop_encodes_rep() {
        use core::num::NonZeroU16;

        let data = vec![0x80u8; 4];
        let (config, _) = config_and_activate_payloads(Modulation {
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });
        assert_eq!(&config[8..10], &9u16.to_le_bytes());
    }

    #[test]
    fn later_writes_the_bank_without_changing_it() {
        let data = vec![0x80u8; 4];
        let datagrams = build(
            1,
            Modulation {
                bank: ModulationBank::B1,
                transition_mode: TransitionMode::Later,
                ..Modulation::new(SamplingConfig::FREQ_4K, &data)
            },
        )
        .unwrap();

        assert_eq!(
            cmds(&datagrams),
            [Cmd::WriteModulationBuffer, Cmd::ConfigModulation],
            "write + config, no activate"
        );
        let config = payload(&datagrams, 1, 0);
        assert_eq!(config[0], 1, "bank B1");
        assert_eq!(&config[4..8], &4u32.to_le_bytes());
    }

    #[test]
    fn later_skips_the_loop_behavior_constraint() {
        use core::num::NonZeroU16;

        let data = vec![0x80u8; 4];
        let result = build(
            1,
            Modulation {
                bank: ModulationBank::B1,
                loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
                transition_mode: TransitionMode::Later,
                ..Modulation::new(SamplingConfig::FREQ_4K, &data)
            },
        );
        assert!(result.is_ok(), "no transition, so no constraint applies");
    }
}

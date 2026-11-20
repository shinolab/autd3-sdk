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

impl Command for Modulation<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
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
            })?
            .push(ActivateModulationBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::protocol::Cmd;
    use crate::test_utils::{build, cmds, payload, waits};

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
        assert_eq!(&config[3..7], &20u32.to_le_bytes(), "size");
        assert_eq!(activate[0], 1, "bank B1");
    }

    #[test]
    fn modulation_pads_an_odd_tail_to_a_word() {
        let data = [0x11u8, 0x22, 0x33];
        let frames = build(1, Modulation::new(SamplingConfig::FREQ_4K, &data)).unwrap();
        assert_eq!(&payload(&frames, 0, 0)[5..], &[0x11, 0x22, 0x33, 0x00]);
        assert_eq!(&payload(&frames, 1, 0)[3..7], &3u32.to_le_bytes());
    }

    #[test]
    fn modulation_fills_the_whole_buffer() {
        let data = vec![0x55; crate::params::MOD_BUFFER_SAMPLES];
        let frames = build(1, Modulation::new(SamplingConfig::FREQ_4K, &data)).unwrap();
        let cmds = cmds(&frames);
        let (writes, rest) = cmds.split_at(cmds.len() - 2);
        assert!(writes.len() > 1);
        assert!(writes.iter().all(|&c| c == Cmd::WriteModulationBuffer));
        assert_eq!(rest, [Cmd::ConfigModulation, Cmd::ActivateModulationBank]);
        let waits = waits(&frames);
        assert_eq!(waits.iter().filter(|&&w| w).count(), 1);
        assert_eq!(waits.last(), Some(&true));
    }

    #[test]
    fn modulation_rejects_empty_data() {
        assert!(matches!(
            build(1, Modulation::new(SamplingConfig::FREQ_4K, &[])),
            Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
        ));
    }

    #[test]
    fn modulation_defaults_to_infinite_loop() {
        let data = vec![0x80u8; 4];
        let (config, _) =
            config_and_activate_payloads(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(&config[7..9], &0xFFFFu16.to_le_bytes());
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
        assert_eq!(&config[7..9], &9u16.to_le_bytes());
    }
}

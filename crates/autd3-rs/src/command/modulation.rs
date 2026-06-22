use super::Command;
use crate::datagram::DatagramBuilder;
use crate::operation::{ChangeModulationBank, ConfigModulation, WriteModulationBuffer};
use crate::value::{ModulationBank, SamplingConfig, TransitionMode};

#[derive(Clone, Copy, Debug)]
pub struct Modulation<'a> {
    pub bank: ModulationBank,
    pub config: SamplingConfig,
    pub data: &'a [u8],
}

impl<'a> Modulation<'a> {
    #[must_use]
    pub fn new(config: SamplingConfig, data: &'a [u8]) -> Self {
        Self::with_bank(ModulationBank::B0, config, data)
    }

    #[must_use]
    pub fn with_bank(bank: ModulationBank, config: SamplingConfig, data: &'a [u8]) -> Self {
        Self { bank, config, data }
    }
}

impl<'a> Command<'a> for Modulation<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let divider = self.config.divide().unwrap_or(0);
        let size = u32::try_from(self.data.len()).unwrap_or(u32::MAX);
        builder
            .push(WriteModulationBuffer {
                bank: self.bank,
                offset: 0,
                data: self.data,
            })
            .push(ConfigModulation {
                bank: self.bank,
                divider,
                size,
            })
            .push(ChangeModulationBank {
                bank: self.bank,
                transition_mode: TransitionMode::Immediate,
                transition_value: 0,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cmd;

    #[test]
    fn modulation_expands_with_size_from_data() {
        let data = vec![0x80u8; 20];
        let mut b = DatagramBuilder::new(1);
        b.push(Modulation::with_bank(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            &data,
        ));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3);
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteModulationBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigModulation);
        assert_eq!(cfg.datagrams()[0].payload[0], 1, "bank B1");
        assert_eq!(&cfg.datagrams()[0].payload[4..8], &20u32.to_le_bytes());

        let chg = datagrams.frame(2).unwrap();
        assert_eq!(chg.datagrams()[0].cmd, Cmd::ChangeModulationBank);
        assert_eq!(chg.datagrams()[0].payload[0], 1, "bank B1");
        assert_eq!(chg.datagrams()[0].payload[1], 0xFF, "IMMEDIATE");
    }
}

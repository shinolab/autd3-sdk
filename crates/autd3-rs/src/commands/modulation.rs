use super::Command;
use crate::commands::operation::{
    ChangeModulationBank, ConfigModulation, WriteModulationBuffer, WriteModulationFused,
};
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
        let size = self.data.len();
        if WriteModulationFused::fits_single_frame(size) {
            builder.push(WriteModulationFused {
                bank: self.bank,
                data: self.data,
                config: self.config,
                loop_behavior: self.loop_behavior,
                transition_mode: self.transition_mode,
            });
            return;
        }
        builder
            .push(WriteModulationBuffer {
                bank: self.bank,
                offset: 0,
                data: self.data,
            })
            .push(ConfigModulation {
                bank: self.bank,
                config: self.config,
                size,
                loop_behavior: self.loop_behavior,
            })
            .push(ChangeModulationBank {
                bank: self.bank,
                transition_mode: self.transition_mode,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cmd;
    use crate::test_utils::test_geometry_arc;

    use crate::commands::operation::MOD_FUSED_MAX_DATA_LEN;

    fn fused_payload(m: Modulation<'_>) -> [u8; crate::protocol::PAYLOAD_BYTES] {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(m);
        let datagrams = b.build().unwrap();
        assert_eq!(datagrams.len(), 1, "short modulation fuses into 1 frame");
        let f = datagrams.frame(0).unwrap();
        assert_eq!(f.datagrams()[0].cmd, Cmd::WriteModulationFused);
        f.datagrams()[0].payload
    }

    #[test]
    fn modulation_expands_with_size_from_data() {
        let data = vec![0x80u8; 20];
        let payload = fused_payload(Modulation::with_bank(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            &data,
        ));

        assert_eq!(payload[0], 1, "bank B1");
        assert_eq!(payload[1], 0xFF, "IMMEDIATE");
        assert_eq!(&payload[4..8], &20u32.to_le_bytes(), "size");
        assert_eq!(&payload[10..12], &20u16.to_le_bytes(), "data_len");
    }

    #[test]
    fn modulation_defaults_to_infinite_loop() {
        let data = vec![0x80u8; 4];
        let payload = fused_payload(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(&payload[8..10], &0xFFFFu16.to_le_bytes());
    }

    #[test]
    fn modulation_defaults_to_immediate_transition() {
        let data = vec![0x80u8; 4];
        let payload = fused_payload(Modulation::new(SamplingConfig::FREQ_4K, &data));
        assert_eq!(payload[1], 0xFF, "IMMEDIATE");
        assert_eq!(&payload[12..20], &0u64.to_le_bytes());
    }

    #[test]
    fn modulation_transition_mode_encodes_into_fused_frame() {
        use crate::value::{DcSysTime, TransitionMode};

        let data = vec![0x80u8; 4];
        let payload = fused_payload(Modulation {
            transition_mode: TransitionMode::SysTime {
                time: DcSysTime::from_nanos(0xDEAD_BEEF),
                margin: None,
            },
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });

        assert_eq!(payload[1], 0x01, "SYS_TIME");
        assert_eq!(&payload[12..20], &0xDEAD_BEEFu64.to_le_bytes());
    }

    #[test]
    fn modulation_finite_loop_encodes_rep() {
        use crate::value::LoopBehavior;
        use core::num::NonZeroU16;

        let data = vec![0x80u8; 4];
        let payload = fused_payload(Modulation {
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
            ..Modulation::new(SamplingConfig::FREQ_4K, &data)
        });
        assert_eq!(&payload[8..10], &9u16.to_le_bytes());
    }

    #[test]
    fn long_modulation_falls_back_to_the_three_frame_path() {
        let data = vec![0x80u8; MOD_FUSED_MAX_DATA_LEN + 1];
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(Modulation::with_bank(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            &data,
        ));
        let datagrams = b.build().unwrap();

        assert_eq!(datagrams.len(), 3, "write + config + change");
        assert_eq!(
            datagrams.frame(0).unwrap().datagrams()[0].cmd,
            Cmd::WriteModulationBuffer
        );
        let cfg = datagrams.frame(1).unwrap();
        assert_eq!(cfg.datagrams()[0].cmd, Cmd::ConfigModulation);
        let size = u32::try_from(data.len()).unwrap();
        assert_eq!(&cfg.datagrams()[0].payload[4..8], &size.to_le_bytes());
        assert_eq!(
            datagrams.frame(2).unwrap().datagrams()[0].cmd,
            Cmd::ChangeModulationBank
        );
    }
}

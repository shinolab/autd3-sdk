use crate::error::{Error, PayloadError};
use crate::params::MOD_BUFFER_SAMPLES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::ModulationBank;

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct ConfigModulation {
    pub bank: ModulationBank,
    pub divider: u16,
    pub size: u32,
}

impl Operation for ConfigModulation {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        if self.divider == 0 {
            return Err(Error::InvalidPayload(PayloadError::ModulationDividerZero));
        }
        if self.size == 0 || self.size as usize > MOD_BUFFER_SAMPLES {
            return Err(Error::InvalidPayload(
                PayloadError::ModulationSizeOutOfRange {
                    size: self.size,
                    max: MOD_BUFFER_SAMPLES,
                },
            ));
        }
        out[0] = self.bank.as_u8();
        out[2..4].copy_from_slice(&self.divider.to_le_bytes());
        out[4..8].copy_from_slice(&self.size.to_le_bytes());
        Ok(Cmd::ConfigModulation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(op: ConfigModulation) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out)?;
        Ok((cmd, out))
    }

    #[test]
    fn config_modulation_lays_out_fields() {
        let (cmd, payload) = encode(ConfigModulation {
            bank: ModulationBank::B1,
            divider: 10,
            size: 4000,
        })
        .unwrap();

        assert_eq!(cmd, Cmd::ConfigModulation);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[2..4], &10u16.to_le_bytes());
        assert_eq!(&payload[4..8], &4000u32.to_le_bytes());
        assert!(payload[8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn config_modulation_rejects_invalid_fields() {
        let base = ConfigModulation {
            bank: ModulationBank::B0,
            divider: 1,
            size: 1,
        };
        assert!(matches!(
            encode(ConfigModulation { divider: 0, ..base }),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(ConfigModulation { size: 0, ..base }),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(ConfigModulation {
                size: u32::try_from(MOD_BUFFER_SAMPLES + 1).unwrap(),
                ..base
            }),
            Err(Error::InvalidPayload(_))
        ));
        assert!(
            encode(ConfigModulation {
                size: u32::try_from(MOD_BUFFER_SAMPLES).unwrap(),
                ..base
            })
            .is_ok()
        );
    }
}

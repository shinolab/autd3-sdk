use crate::error::{Error, PayloadError};
use crate::params::{EMISSION_MAX_INDICES, MAX_FOCI_TOTAL, NUM_FOCI_MAX};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, PatternDataType};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct ConfigPattern {
    pub bank: PatternBank,
    pub divider: u16,
    pub size: u32,
    pub data_type: PatternDataType,
}

impl Operation for ConfigPattern {
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
            return Err(Error::InvalidPayload(PayloadError::PatternDividerZero));
        }
        if self.size == 0 {
            return Err(Error::InvalidPayload(PayloadError::PatternSizeZero));
        }
        let (type_byte, num_foci, sound_speed) = match self.data_type {
            PatternDataType::Foci {
                num_foci,
                sound_speed,
            } => {
                if num_foci == 0 || num_foci > NUM_FOCI_MAX {
                    return Err(Error::InvalidPayload(PayloadError::NumFociOutOfRange {
                        num_foci,
                        max: NUM_FOCI_MAX,
                    }));
                }
                if self.size as usize > MAX_FOCI_TOTAL / usize::from(num_foci) {
                    return Err(Error::InvalidPayload(PayloadError::StmFociExceedCapacity {
                        size: self.size,
                        num_foci,
                        capacity: MAX_FOCI_TOTAL,
                    }));
                }
                if sound_speed == 0 {
                    return Err(Error::InvalidPayload(PayloadError::SoundSpeedZero));
                }
                (0u8, num_foci, sound_speed)
            }
            PatternDataType::Raw => {
                if self.size as usize > EMISSION_MAX_INDICES {
                    return Err(Error::InvalidPayload(PayloadError::StmSizeOutOfRange {
                        size: self.size,
                        max: EMISSION_MAX_INDICES,
                    }));
                }
                (1u8, 0, 0)
            }
        };
        out[0] = self.bank.as_u8();
        out[1] = type_byte;
        out[2..4].copy_from_slice(&self.divider.to_le_bytes());
        out[4..8].copy_from_slice(&self.size.to_le_bytes());
        out[8] = num_foci;
        out[10..12].copy_from_slice(&sound_speed.to_le_bytes());
        Ok(Cmd::ConfigPattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(op: ConfigPattern) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out)?;
        Ok((cmd, out))
    }

    #[test]
    fn config_pattern_lays_out_raw_fields() {
        let (cmd, payload) = encode(ConfigPattern {
            bank: PatternBank::B0,
            divider: 2,
            size: 1024,
            data_type: PatternDataType::Raw,
        })
        .unwrap();

        assert_eq!(cmd, Cmd::ConfigPattern);
        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 1, "RawEmissions wire value");
        assert_eq!(&payload[2..4], &2u16.to_le_bytes());
        assert_eq!(&payload[4..8], &1024u32.to_le_bytes());
        assert_eq!(payload[8], 0);
        assert_eq!(&payload[10..12], &0u16.to_le_bytes());
    }

    #[test]
    fn config_pattern_lays_out_foci_fields() {
        let (_cmd, payload) = encode(ConfigPattern {
            bank: PatternBank::B1,
            divider: 1,
            size: 8192,
            data_type: PatternDataType::Foci {
                num_foci: 8,
                sound_speed: 340,
            },
        })
        .unwrap();

        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0, "Foci wire value");
        assert_eq!(&payload[4..8], &8192u32.to_le_bytes());
        assert_eq!(payload[8], 8);
        assert_eq!(&payload[10..12], &340u16.to_le_bytes());
    }

    #[test]
    fn config_pattern_rejects_invalid_fields() {
        let raw = |size: u32| ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size,
            data_type: PatternDataType::Raw,
        };
        assert!(matches!(encode(raw(0)), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            encode(raw(u32::try_from(EMISSION_MAX_INDICES + 1).unwrap())),
            Err(Error::InvalidPayload(_))
        ));

        let foci = |size: u32, num_foci: u8, sound_speed: u16| ConfigPattern {
            bank: PatternBank::B0,
            divider: 1,
            size,
            data_type: PatternDataType::Foci {
                num_foci,
                sound_speed,
            },
        };
        assert!(matches!(
            encode(foci(1, 0, 340)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(foci(1, NUM_FOCI_MAX + 1, 340)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(foci(u32::try_from(MAX_FOCI_TOTAL / 8 + 1).unwrap(), 8, 340)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(foci(1, 1, 0)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(encode(foci(u32::try_from(MAX_FOCI_TOTAL / 8).unwrap(), 8, 340)).is_ok());
    }
}

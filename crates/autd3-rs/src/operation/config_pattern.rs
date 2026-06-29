use crate::error::{Error, PayloadError};
use crate::mirror::FirmwareState;
use crate::params::{EMISSION_MAX_INDICES, MAX_FOCI_TOTAL, NUM_FOCI_MAX};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{LoopBehavior, PatternBank, PatternDataType, SamplingConfig};

use super::{Distribution, Operation, silencer_constraint};

#[derive(Clone, Copy, Debug)]
pub struct ConfigPattern {
    pub bank: PatternBank,
    pub config: SamplingConfig,
    pub size: usize,
    pub data_type: PatternDataType,
    pub loop_behavior: LoopBehavior,
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
        let divider = self
            .config
            .divide()
            .map_err(|e| Error::InvalidPayload(PayloadError::from(e)))?;
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
                if self.size > MAX_FOCI_TOTAL / usize::from(num_foci) {
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
                if self.size > EMISSION_MAX_INDICES {
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
        out[2..4].copy_from_slice(&divider.to_le_bytes());
        out[4..8].copy_from_slice(
            &u32::try_from(self.size)
                .expect("bounded by capacity checks")
                .to_le_bytes(),
        );
        out[8] = num_foci;
        out[10..12].copy_from_slice(&sound_speed.to_le_bytes());
        out[12..14].copy_from_slice(&self.loop_behavior.rep().to_le_bytes());
        Ok(Cmd::ConfigPattern)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let divider = self
            .config
            .divide()
            .map_err(|e| Error::InvalidPayload(PayloadError::from(e)))?;
        if let Err(v) = state.silencer.check_pattern_div(divider) {
            return Err(silencer_constraint(device, v));
        }
        state.silencer.note_pattern_div(self.bank.as_u8(), divider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU16;

    fn encode(op: ConfigPattern) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out)?;
        Ok((cmd, out))
    }

    #[test]
    fn config_pattern_lays_out_raw_fields() {
        let (cmd, payload) = encode(ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::Divide(NonZeroU16::new(2).unwrap()),
            size: 1024,
            data_type: PatternDataType::Raw,
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(8).unwrap()),
        })
        .unwrap();

        assert_eq!(cmd, Cmd::ConfigPattern);
        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 1, "RawEmissions wire value");
        assert_eq!(&payload[2..4], &2u16.to_le_bytes());
        assert_eq!(&payload[4..8], &1024u32.to_le_bytes());
        assert_eq!(payload[8], 0);
        assert_eq!(&payload[10..12], &0u16.to_le_bytes());
        assert_eq!(&payload[12..14], &7u16.to_le_bytes());
    }

    #[test]
    fn config_pattern_lays_out_foci_fields() {
        let (_cmd, payload) = encode(ConfigPattern {
            bank: PatternBank::B1,
            config: SamplingConfig::Divide(NonZeroU16::MIN),
            size: 8192,
            data_type: PatternDataType::Foci {
                num_foci: 8,
                sound_speed: 340,
            },
            loop_behavior: LoopBehavior::Infinite,
        })
        .unwrap();

        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0, "Foci wire value");
        assert_eq!(&payload[4..8], &8192u32.to_le_bytes());
        assert_eq!(payload[8], 8);
        assert_eq!(&payload[10..12], &340u16.to_le_bytes());
        assert_eq!(&payload[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
    }

    #[test]
    fn config_pattern_rejects_invalid_fields() {
        let raw = |size: usize| ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::Divide(NonZeroU16::MIN),
            size,
            data_type: PatternDataType::Raw,
            loop_behavior: LoopBehavior::Infinite,
        };
        assert!(matches!(encode(raw(0)), Err(Error::InvalidPayload(_))));
        assert!(
            matches!(
                encode(ConfigPattern {
                    config: SamplingConfig::Period(core::time::Duration::from_nanos(1)),
                    ..raw(1)
                }),
                Err(Error::InvalidPayload(_))
            ),
            "an unrepresentable sampling config is rejected"
        );
        assert!(matches!(
            encode(raw(EMISSION_MAX_INDICES + 1)),
            Err(Error::InvalidPayload(_))
        ));

        let foci = |size: usize, num_foci: u8, sound_speed: u16| ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::Divide(NonZeroU16::MIN),
            size,
            data_type: PatternDataType::Foci {
                num_foci,
                sound_speed,
            },
            loop_behavior: LoopBehavior::Infinite,
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
            encode(foci(MAX_FOCI_TOTAL / 8 + 1, 8, 340)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(foci(1, 1, 0)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(encode(foci(MAX_FOCI_TOTAL / 8, 8, 340)).is_ok());
    }
}

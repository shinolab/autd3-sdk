use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::Velocity;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{LoopBehavior, PatternBank, SamplingConfig};

use super::{Encoded, Operation, encode_fixed};

#[derive(Clone, Copy, Debug)]
pub struct ConfigPattern {
    pub bank: PatternBank,
    pub config: SamplingConfig,
    pub size: usize,
    pub loop_behavior: LoopBehavior,
}

#[derive(Clone, Copy, Debug)]
pub struct ConfigFociStm {
    pub bank: PatternBank,
    pub config: SamplingConfig,
    pub size: usize,
    pub num_foci: u8,
    pub sound_speed: Velocity,
    pub loop_behavior: LoopBehavior,
}

impl crate::sealed::Sealed for ConfigPattern {}

impl Operation for ConfigPattern {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let payload = ConfigPatternPayload::raw(
            self.bank,
            self.config.divide()?,
            self.size,
            self.loop_behavior.rep(),
        )?;
        Ok(encode_fixed(out, Cmd::ConfigPattern, &payload))
    }
}

impl crate::sealed::Sealed for ConfigFociStm {}

impl Operation for ConfigFociStm {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let payload = ConfigPatternPayload::foci(
            self.bank,
            self.config.divide()?,
            self.size,
            self.num_foci,
            self.sound_speed.m_s(),
            self.loop_behavior.rep(),
        )?;
        Ok(encode_fixed(out, Cmd::ConfigPattern, &payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::params::{EMISSION_MAX_INDICES, MAX_FOCI_TOTAL, NUM_FOCI_MAX};
    use crate::test_utils::encode;
    use core::num::NonZeroU16;

    const ENCODED: Encoded = Encoded::new(Cmd::ConfigPattern, size_of::<ConfigPatternPayload>());

    #[test]
    fn config_pattern_lays_out_raw_fields() {
        let (encoded, payload) = encode(&ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::new(NonZeroU16::new(2).unwrap()),
            size: 1024,
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(8).unwrap()),
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 1, "RawEmissions wire value");
        assert_eq!(&payload[2..4], &2u16.to_le_bytes());
        assert_eq!(&payload[4..8], &1024u32.to_le_bytes());
        assert_eq!(payload[8], 0);
        assert_eq!(&payload[10..12], &0u16.to_le_bytes());
        assert_eq!(&payload[12..14], &7u16.to_le_bytes());
    }

    #[test]
    fn config_foci_stm_lays_out_foci_fields() {
        let (encoded, payload) = encode(&ConfigFociStm {
            bank: PatternBank::B1,
            config: SamplingConfig::new(NonZeroU16::MIN),
            size: 8192,
            num_foci: 8,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Infinite,
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0, "Foci wire value");
        assert_eq!(&payload[4..8], &8192u32.to_le_bytes());
        assert_eq!(payload[8], 8);
        assert_eq!(&payload[10..12], &21760u16.to_le_bytes(), "340 m/s * 64");
        assert_eq!(&payload[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
    }

    #[test]
    fn config_pattern_rejects_invalid_raw_fields() {
        let raw = |size: usize| ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::new(NonZeroU16::MIN),
            size,
            loop_behavior: LoopBehavior::Infinite,
        };
        assert!(matches!(encode(&raw(0)), Err(Error::InvalidPayload(_))));
        assert!(
            matches!(
                encode(&ConfigPattern {
                    config: SamplingConfig::new(core::time::Duration::from_nanos(1)),
                    ..raw(1)
                }),
                Err(Error::InvalidPayload(_))
            ),
            "an unrepresentable sampling config is rejected"
        );
        assert!(matches!(
            encode(&raw(EMISSION_MAX_INDICES + 1)),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn config_pattern_allows_a_single_index_only_for_an_infinite_loop() {
        let raw = |size: usize, loop_behavior: LoopBehavior| ConfigPattern {
            bank: PatternBank::B0,
            config: SamplingConfig::new(NonZeroU16::MIN),
            size,
            loop_behavior,
        };
        let finite = LoopBehavior::Finite(NonZeroU16::new(4).unwrap());

        assert_eq!(
            encode(&raw(1, LoopBehavior::Infinite)).unwrap().0,
            ENCODED,
            "a static pattern is a single index"
        );
        assert!(
            matches!(encode(&raw(1, finite)), Err(Error::InvalidPayload(_))),
            "a single index never advances, so a finite loop would never end"
        );
        assert_eq!(encode(&raw(2, finite)).unwrap().0, ENCODED);
    }

    #[test]
    fn config_foci_stm_rejects_invalid_fields() {
        let foci = |size: usize, num_foci: u8, sound_speed: Velocity| ConfigFociStm {
            bank: PatternBank::B0,
            config: SamplingConfig::new(NonZeroU16::MIN),
            size,
            num_foci,
            sound_speed,
            loop_behavior: LoopBehavior::Infinite,
        };
        let v = Velocity::from_m_s(340.0);
        assert!(
            matches!(encode(&foci(1, 1, v)), Err(Error::InvalidPayload(_))),
            "a single-sample STM never advances its index"
        );
        assert!(matches!(
            encode(&foci(2, 0, v)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(&foci(2, NUM_FOCI_MAX + 1, v)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(&foci(MAX_FOCI_TOTAL / 8 + 1, 8, v)),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(&foci(2, 1, Velocity::from_m_s(0.0))),
            Err(Error::InvalidPayload(_))
        ));
        assert_eq!(encode(&foci(MAX_FOCI_TOTAL / 8, 8, v)).unwrap().0, ENCODED);
        assert_eq!(
            encode(&foci(2, 1, Velocity::from_m_s(1023.0))).unwrap().0,
            ENCODED,
            "1023 m/s * 64 still fits 16 bits"
        );
        for too_fast in [1024.0, 1500.0, f32::INFINITY] {
            assert!(
                matches!(
                    encode(&foci(2, 1, Velocity::from_m_s(too_fast))),
                    Err(Error::InvalidPayload(
                        PayloadError::SoundSpeedTooLarge { .. }
                    ))
                ),
                "{too_fast} m/s must not saturate silently"
            );
        }
    }
}

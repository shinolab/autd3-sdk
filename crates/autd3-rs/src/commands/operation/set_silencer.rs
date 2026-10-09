use core::num::NonZeroU16;
use core::time::Duration;

use autd3_cpu_wire::payload::SilencerPayload;

use crate::common::ULTRASOUND_PERIOD;
use crate::error::Error;
use crate::geometry::Device;
use crate::params::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation, encode_fixed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedCompletionTime {
    pub intensity: Duration,
    pub phase: Duration,
    pub strict_mode: bool,
}

impl Default for FixedCompletionTime {
    fn default() -> Self {
        Self {
            intensity: ULTRASOUND_PERIOD * u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY),
            phase: ULTRASOUND_PERIOD * u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE),
            strict_mode: true,
        }
    }
}

impl FixedCompletionTime {
    fn payload(&self) -> Result<SilencerPayload, Error> {
        Ok(SilencerPayload::fixed_completion_time(
            self.intensity,
            self.phase,
            self.strict_mode,
        )?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedUpdateRate {
    pub intensity: NonZeroU16,
    pub phase: NonZeroU16,
}

impl FixedUpdateRate {
    fn payload(self) -> SilencerPayload {
        SilencerPayload::fixed_update_rate(self.intensity, self.phase)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SilencerConfig {
    FixedCompletionTime(FixedCompletionTime),
    FixedUpdateRate(FixedUpdateRate),
}

impl Default for SilencerConfig {
    fn default() -> Self {
        Self::FixedCompletionTime(FixedCompletionTime::default())
    }
}

impl From<FixedCompletionTime> for SilencerConfig {
    fn from(config: FixedCompletionTime) -> Self {
        Self::FixedCompletionTime(config)
    }
}

impl From<FixedUpdateRate> for SilencerConfig {
    fn from(config: FixedUpdateRate) -> Self {
        Self::FixedUpdateRate(config)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetSilencer {
    pub config: SilencerConfig,
}

impl SetSilencer {
    #[must_use]
    pub fn new(config: impl Into<SilencerConfig>) -> Self {
        Self {
            config: config.into(),
        }
    }

    #[must_use]
    pub const fn disable() -> Self {
        Self {
            config: SilencerConfig::FixedCompletionTime(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD,
                phase: ULTRASOUND_PERIOD,
                strict_mode: false,
            }),
        }
    }
}

impl crate::sealed::Sealed for SetSilencer {}

impl Operation for SetSilencer {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let payload = match self.config {
            SilencerConfig::FixedCompletionTime(config) => config.payload()?,
            SilencerConfig::FixedUpdateRate(config) => config.payload(),
        };
        Ok(encode_fixed(out, Cmd::SetSilencer, &payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::params::{SILENCER_DEFAULT_UPDATE_RATE, SilencerFlags};
    use crate::test_utils::encode;
    use rstest::rstest;

    const ENCODED: Encoded = Encoded::new(Cmd::SetSilencer, size_of::<SilencerPayload>());

    fn nz(v: u16) -> NonZeroU16 {
        NonZeroU16::new(v).unwrap()
    }

    #[test]
    fn fixed_completion_time_lays_out_fields() {
        let (encoded, payload) = encode(&SetSilencer::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 5,
            phase: ULTRASOUND_PERIOD * 7,
            strict_mode: true,
        }))
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], SilencerFlags::STRICT_MODE.bits());
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[2..4], &SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[4..6], &SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[6..8], &5u16.to_le_bytes());
        assert_eq!(&payload[8..10], &7u16.to_le_bytes());
        assert!(payload[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn default_is_fixed_completion_time_10_40_strict() {
        assert_eq!(
            SetSilencer::default(),
            SetSilencer::new(FixedCompletionTime::default())
        );
        let (encoded, payload) = encode(&SetSilencer::default()).unwrap();
        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], SilencerFlags::STRICT_MODE.bits());
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn disable_is_one_step_non_strict() {
        let (encoded, payload) = encode(&SetSilencer::disable()).unwrap();
        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 0);
        assert_eq!(&payload[6..8], &1u16.to_le_bytes());
        assert_eq!(&payload[8..10], &1u16.to_le_bytes());
    }

    #[test]
    fn fixed_completion_time_non_strict_clears_flag() {
        let (encoded, payload) = encode(&SetSilencer::new(FixedCompletionTime {
            strict_mode: false,
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 0);
    }

    #[test]
    fn fixed_update_rate_sets_mode_flag() {
        let (encoded, payload) = encode(&SetSilencer::new(FixedUpdateRate {
            intensity: nz(8),
            phase: nz(16),
        }))
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], SilencerFlags::FIXED_UPDATE_RATE_MODE.bits());
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[2..4], &8u16.to_le_bytes());
        assert_eq!(&payload[4..6], &16u16.to_le_bytes());
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
        assert!(payload[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_non_multiple_completion_time() {
        assert!(matches!(
            encode(&SetSilencer::new(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD + Duration::from_nanos(1),
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            })),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeNotMultiple(_)
            ))
        ));
    }

    #[rstest]
    #[case::zero(Duration::ZERO)]
    #[case::beyond_the_wire_range(ULTRASOUND_PERIOD * 65536)]
    fn rejects_out_of_range_completion_time(#[case] intensity: Duration) {
        assert!(matches!(
            encode(&SetSilencer::new(FixedCompletionTime {
                intensity,
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            })),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeOutOfRange(_)
            ))
        ));
    }
}

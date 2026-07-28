use core::num::NonZeroU16;
use core::time::Duration;

use autd3_rs_core::common::ULTRASOUND_FREQ;
use autd3_rs_core::geometry::Device;
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::Tag;
use crate::legacy::wire::params::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, SILENCER_FLAG_STRICT_MODE,
};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct ConfigSilencer {
    tag: u8,
    flag: u8,
    value_intensity: u16,
    value_phase: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SilencerConfig {
    FixedUpdateRate {
        intensity: NonZeroU16,
        phase: NonZeroU16,
    },
    FixedCompletionSteps {
        intensity: NonZeroU16,
        phase: NonZeroU16,
        strict: bool,
    },
    FixedCompletionTime {
        intensity: Duration,
        phase: Duration,
        strict: bool,
    },
}

impl Default for SilencerConfig {
    fn default() -> Self {
        Self::default_with_strict(true)
    }
}

impl SilencerConfig {
    #[must_use]
    pub fn default_non_strict() -> Self {
        Self::default_with_strict(false)
    }

    fn default_with_strict(strict: bool) -> Self {
        Self::FixedCompletionSteps {
            intensity: NonZeroU16::new(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY)
                .expect("the default completion steps are non-zero"),
            phase: NonZeroU16::new(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE)
                .expect("the default completion steps are non-zero"),
            strict,
        }
    }
}

fn completion_steps(value: Duration) -> Result<u16, PayloadError> {
    const NANOSEC: u128 = 1_000_000_000;
    let v = value.as_nanos() * u128::from(ULTRASOUND_FREQ.hz());
    if !v.is_multiple_of(NANOSEC) {
        return Err(PayloadError::SilencerCompletionTimeNotMultiple(value));
    }
    let v = v / NANOSEC;
    if v == 0 || v > u128::from(u16::MAX) {
        return Err(PayloadError::SilencerCompletionTimeOutOfRange(value));
    }
    u16::try_from(v).map_err(|_| PayloadError::SilencerCompletionTimeOutOfRange(value))
}

impl SilencerConfig {
    fn encode(self) -> Result<(u8, u16, u16), PayloadError> {
        match self {
            SilencerConfig::FixedUpdateRate { intensity, phase } => Ok((
                SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
                intensity.get(),
                phase.get(),
            )),
            SilencerConfig::FixedCompletionSteps {
                intensity,
                phase,
                strict,
            } => Ok((strict_flag(strict), intensity.get(), phase.get())),
            SilencerConfig::FixedCompletionTime {
                intensity,
                phase,
                strict,
            } => Ok((
                strict_flag(strict),
                completion_steps(intensity)?,
                completion_steps(phase)?,
            )),
        }
    }
}

const fn strict_flag(strict: bool) -> u8 {
    if strict { SILENCER_FLAG_STRICT_MODE } else { 0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Silencer {
    config: SilencerConfig,
    done: bool,
}

impl Silencer {
    #[must_use]
    pub const fn new(config: SilencerConfig) -> Self {
        Self {
            config,
            done: false,
        }
    }
}

impl LegacyOperation for Silencer {
    fn required_size(&self, _device: &Device) -> usize {
        size_of::<ConfigSilencer>()
    }

    fn pack(&mut self, _device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let (flag, value_intensity, value_phase) = self.config.encode()?;
        let msg = ConfigSilencer {
            tag: Tag::Silencer.as_u8(),
            flag,
            value_intensity,
            value_phase,
        };
        tx[..size_of::<ConfigSilencer>()].copy_from_slice(msg.as_bytes());
        self.done = true;
        Ok(size_of::<ConfigSilencer>())
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::common::ULTRASOUND_PERIOD;
    use autd3_rs_core::geometry::{Autd3, Geometry};

    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default()])
    }

    fn nz(v: u16) -> NonZeroU16 {
        NonZeroU16::new(v).unwrap()
    }

    fn packed(config: SilencerConfig) -> [u8; 6] {
        let geo = geometry();
        let mut op = Silencer::new(config);
        assert_eq!(op.required_size(&geo[0]), 6);
        let mut tx = [0u8; 6];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 6);
        assert!(op.is_done());
        tx
    }

    #[test]
    fn fixed_update_rate_sets_only_the_rate_flag() {
        let tx = packed(SilencerConfig::FixedUpdateRate {
            intensity: nz(0x1234),
            phase: nz(0x5678),
        });
        assert_eq!(tx[0], Tag::Silencer.as_u8());
        assert_eq!(tx[1], SILENCER_FLAG_FIXED_UPDATE_RATE_MODE);
        assert_eq!(&tx[2..4], &0x1234u16.to_le_bytes());
        assert_eq!(&tx[4..6], &0x5678u16.to_le_bytes());
    }

    #[test]
    fn completion_steps_carry_the_strict_flag() {
        for (strict, flag) in [(true, SILENCER_FLAG_STRICT_MODE), (false, 0)] {
            let tx = packed(SilencerConfig::FixedCompletionSteps {
                intensity: nz(0x12),
                phase: nz(0x34),
                strict,
            });
            assert_eq!(tx[1], flag);
            assert_eq!(&tx[2..4], &0x12u16.to_le_bytes());
            assert_eq!(&tx[4..6], &0x34u16.to_le_bytes());
        }
    }

    #[test]
    fn completion_time_converts_to_ultrasound_periods() {
        let tx = packed(SilencerConfig::FixedCompletionTime {
            intensity: 10 * ULTRASOUND_PERIOD,
            phase: 40 * ULTRASOUND_PERIOD,
            strict: true,
        });
        assert_eq!(&tx[2..4], &10u16.to_le_bytes());
        assert_eq!(&tx[4..6], &40u16.to_le_bytes());
    }

    #[test]
    fn completion_time_must_be_a_multiple_of_the_ultrasound_period() {
        let geo = geometry();
        let mut tx = [0u8; 6];
        let err = Silencer::new(SilencerConfig::FixedCompletionTime {
            intensity: Duration::from_micros(1),
            phase: ULTRASOUND_PERIOD,
            strict: true,
        })
        .pack(&geo[0], &mut tx)
        .unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::SilencerCompletionTimeNotMultiple(_))
        ));
    }

    #[test]
    fn completion_time_must_fit_in_u16_periods() {
        let geo = geometry();
        let mut tx = [0u8; 6];
        for value in [Duration::ZERO, 65536 * ULTRASOUND_PERIOD] {
            let err = Silencer::new(SilencerConfig::FixedCompletionTime {
                intensity: value,
                phase: ULTRASOUND_PERIOD,
                strict: true,
            })
            .pack(&geo[0], &mut tx)
            .unwrap_err();
            assert!(matches!(
                err,
                LegacyError::InvalidPayload(PayloadError::SilencerCompletionTimeOutOfRange(_))
            ));
        }
    }

    #[test]
    fn default_matches_the_firmware_boot_state() {
        let tx = packed(SilencerConfig::default());
        assert_eq!(tx[1], SILENCER_FLAG_STRICT_MODE);
        assert_eq!(&tx[2..4], &10u16.to_le_bytes());
        assert_eq!(&tx[4..6], &40u16.to_le_bytes());
    }
}

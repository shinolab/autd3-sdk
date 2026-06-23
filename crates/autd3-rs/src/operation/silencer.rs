use core::num::NonZeroU16;
use core::time::Duration;

use crate::common::{ULTRASOUND_FREQ, ULTRASOUND_PERIOD};
use crate::error::{Error, PayloadError};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

const FLAG_FIXED_UPDATE_RATE: u8 = 1 << 0;
const FLAG_STRICT_MODE: u8 = 1 << 1;

const DEFAULT_COMPLETION_STEPS_INTENSITY: u16 = 10;
const DEFAULT_COMPLETION_STEPS_PHASE: u16 = 40;
const DEFAULT_UPDATE_RATE: u16 = 256;

fn write_payload(
    out: &mut [u8; PAYLOAD_BYTES],
    flag: u8,
    update_rate_intensity: u16,
    update_rate_phase: u16,
    completion_steps_intensity: u16,
    completion_steps_phase: u16,
) {
    out[0] = flag;
    out[2..4].copy_from_slice(&update_rate_intensity.to_le_bytes());
    out[4..6].copy_from_slice(&update_rate_phase.to_le_bytes());
    out[6..8].copy_from_slice(&completion_steps_intensity.to_le_bytes());
    out[8..10].copy_from_slice(&completion_steps_phase.to_le_bytes());
}

fn completion_time_to_steps(value: Duration) -> Result<u16, Error> {
    const NANOSEC: u128 = 1_000_000_000;
    let v = value.as_nanos() * u128::from(ULTRASOUND_FREQ.hz());
    if !v.is_multiple_of(NANOSEC) {
        return Err(Error::InvalidPayload(
            PayloadError::SilencerCompletionTimeNotMultiple(value),
        ));
    }
    let steps = v / NANOSEC;
    if steps == 0 {
        return Err(Error::InvalidPayload(
            PayloadError::SilencerCompletionTimeOutOfRange(value),
        ));
    }
    u16::try_from(steps)
        .map_err(|_| Error::InvalidPayload(PayloadError::SilencerCompletionTimeOutOfRange(value)))
}

mod sealed {
    pub trait Sealed {}
}

pub trait SilencerConfig: sealed::Sealed + Copy {
    #[doc(hidden)]
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedCompletionTime {
    pub intensity: Duration,
    pub phase: Duration,
    pub strict_mode: bool,
}

impl Default for FixedCompletionTime {
    fn default() -> Self {
        Self {
            intensity: ULTRASOUND_PERIOD * u32::from(DEFAULT_COMPLETION_STEPS_INTENSITY),
            phase: ULTRASOUND_PERIOD * u32::from(DEFAULT_COMPLETION_STEPS_PHASE),
            strict_mode: true,
        }
    }
}

impl sealed::Sealed for FixedCompletionTime {}
impl SilencerConfig for FixedCompletionTime {
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let intensity = completion_time_to_steps(self.intensity)?;
        let phase = completion_time_to_steps(self.phase)?;
        let flag = if self.strict_mode {
            FLAG_STRICT_MODE
        } else {
            0
        };
        write_payload(
            out,
            flag,
            DEFAULT_UPDATE_RATE,
            DEFAULT_UPDATE_RATE,
            intensity,
            phase,
        );
        Ok(Cmd::SetSilencer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedUpdateRate {
    pub intensity: NonZeroU16,
    pub phase: NonZeroU16,
}

impl sealed::Sealed for FixedUpdateRate {}
impl SilencerConfig for FixedUpdateRate {
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        write_payload(
            out,
            FLAG_FIXED_UPDATE_RATE,
            self.intensity.get(),
            self.phase.get(),
            DEFAULT_COMPLETION_STEPS_INTENSITY,
            DEFAULT_COMPLETION_STEPS_PHASE,
        );
        Ok(Cmd::SetSilencer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Silencer<T: SilencerConfig> {
    pub config: T,
}

impl<T: SilencerConfig> Silencer<T> {
    #[must_use]
    pub const fn new(config: T) -> Self {
        Self { config }
    }
}

impl Default for Silencer<FixedCompletionTime> {
    fn default() -> Self {
        Self::new(FixedCompletionTime::default())
    }
}

impl<T: SilencerConfig> Operation for Silencer<T> {
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
        self.config.write_payload(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode<T: SilencerConfig>(config: T) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = Silencer::new(config).encode(0, 0, &mut out)?;
        Ok((cmd, out))
    }

    fn nz(v: u16) -> NonZeroU16 {
        NonZeroU16::new(v).unwrap()
    }

    #[test]
    fn fixed_completion_time_lays_out_fields() {
        let (cmd, payload) = encode(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 5,
            phase: ULTRASOUND_PERIOD * 7,
            strict_mode: true,
        })
        .unwrap();

        assert_eq!(cmd, Cmd::SetSilencer);
        assert_eq!(payload[0], FLAG_STRICT_MODE);
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[2..4], &DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[4..6], &DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[6..8], &5u16.to_le_bytes());
        assert_eq!(&payload[8..10], &7u16.to_le_bytes());
        assert!(payload[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn fixed_completion_time_default_is_10_40_strict() {
        let (_cmd, payload) = encode(FixedCompletionTime::default()).unwrap();
        assert_eq!(payload[0], FLAG_STRICT_MODE);
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn silencer_default_is_fixed_completion_time_default() {
        let mut out = [0u8; PAYLOAD_BYTES];
        Silencer::default().encode(0, 0, &mut out).unwrap();
        assert_eq!(out[0], FLAG_STRICT_MODE);
        assert_eq!(&out[6..8], &10u16.to_le_bytes());
        assert_eq!(&out[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn fixed_completion_time_non_strict_clears_flag() {
        let (_cmd, payload) = encode(FixedCompletionTime {
            strict_mode: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(payload[0], 0);
    }

    #[test]
    fn fixed_update_rate_sets_mode_flag() {
        let (cmd, payload) = encode(FixedUpdateRate {
            intensity: nz(8),
            phase: nz(16),
        })
        .unwrap();

        assert_eq!(cmd, Cmd::SetSilencer);
        assert_eq!(payload[0], FLAG_FIXED_UPDATE_RATE);
        assert_eq!(&payload[2..4], &8u16.to_le_bytes());
        assert_eq!(&payload[4..6], &16u16.to_le_bytes());
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn rejects_non_multiple_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD + Duration::from_nanos(1),
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeNotMultiple(_)
            ))
        ));
    }

    #[test]
    fn rejects_zero_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: Duration::ZERO,
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeOutOfRange(_)
            ))
        ));
    }

    #[test]
    fn rejects_out_of_range_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD * 65536,
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeOutOfRange(_)
            ))
        ));
    }
}

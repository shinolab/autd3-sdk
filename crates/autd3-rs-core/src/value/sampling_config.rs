#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use core::{num::NonZeroU16, time::Duration};

use crate::common::{Freq, ULTRASOUND_PERIOD, units::Hz};
use crate::params::ULTRASOUND_FREQ_HZ;

const IS_INTEGER_EPSILON: f64 = 1e-6;

#[doc(hidden)]
#[must_use]
pub const fn is_integer(a: f64) -> bool {
    let dist = 0.5 - (a.fract() - 0.5).abs();
    dist < IS_INTEGER_EPSILON + a.abs() * (f64::EPSILON * 16.0)
}

#[derive(Debug, PartialEq, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum SamplingConfigError {
    #[error("Sampling frequency ({0:?}) must divide the ultrasound frequency")]
    FreqInvalid(Freq),
    #[error("Sampling period ({0:?}) must be a multiple of the ultrasound period")]
    PeriodInvalid(Duration),
    #[error("Sampling frequency ({0:?}) is out of range ([{1:?}, {2:?}])")]
    FreqOutOfRange(Freq, Freq, Freq),
    #[error("Sampling period ({0:?}) is out of range ([{1:?}, {2:?}])")]
    PeriodOutOfRange(Duration, Duration, Duration),
}

const PERIOD_MAX: Duration =
    Duration::from_micros(u16::MAX as u64 * ULTRASOUND_PERIOD.as_micros() as u64);

fn at_least_one(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplingConfig(NonZeroU16);

impl SamplingConfig {
    pub const FREQ_40K: Self = Self(NonZeroU16::MIN);
    pub const FREQ_4K: Self = Self(NonZeroU16::new(10).unwrap());

    #[must_use]
    pub const fn new(divide: NonZeroU16) -> Self {
        Self(divide)
    }

    pub fn from_freq(freq: Freq) -> Result<Self, SamplingConfigError> {
        let freq_max = ULTRASOUND_FREQ_HZ as f32 * Hz;
        let freq_min = freq_max / u16::MAX as f32;
        if !(freq_min..=freq_max).contains(&freq) {
            return Err(SamplingConfigError::FreqOutOfRange(
                freq, freq_min, freq_max,
            ));
        }
        let divide = ULTRASOUND_FREQ_HZ as f32 / freq.hz();
        if !is_integer(divide as f64) {
            return Err(SamplingConfigError::FreqInvalid(freq));
        }
        Ok(Self(at_least_one(divide as u16)))
    }

    pub fn from_period(period: Duration) -> Result<Self, SamplingConfigError> {
        if !(ULTRASOUND_PERIOD..=PERIOD_MAX).contains(&period) {
            return Err(SamplingConfigError::PeriodOutOfRange(
                period,
                ULTRASOUND_PERIOD,
                PERIOD_MAX,
            ));
        }
        if !period
            .as_nanos()
            .is_multiple_of(ULTRASOUND_PERIOD.as_nanos())
        {
            return Err(SamplingConfigError::PeriodInvalid(period));
        }
        Ok(Self(at_least_one(
            (period.as_nanos() / ULTRASOUND_PERIOD.as_nanos()) as u16,
        )))
    }

    #[must_use]
    pub const fn divide(self) -> NonZeroU16 {
        self.0
    }

    #[must_use]
    pub fn freq(self) -> Freq {
        ULTRASOUND_FREQ_HZ as f32 / self.0.get() as f32 * Hz
    }

    #[must_use]
    pub fn period(self) -> Duration {
        ULTRASOUND_PERIOD * u32::from(self.0.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::units::kHz;

    #[test]
    fn is_integer_uses_relative_tolerance() {
        assert!(is_integer(0.0));
        assert!(is_integer(42.0));
        assert!(is_integer(1e12));
        assert!(!is_integer(2.5));
        assert!(!is_integer(1e12 + 0.5));
        assert!(is_integer(1e12 + 1e-4));
    }

    #[test]
    fn a_divider_is_taken_as_is() {
        assert_eq!(SamplingConfig::new(NonZeroU16::MIN).divide().get(), 1);
        assert_eq!(
            SamplingConfig::new(NonZeroU16::MAX).divide().get(),
            u16::MAX
        );
        assert_eq!(SamplingConfig::FREQ_4K.divide().get(), 10);
    }

    #[test]
    fn a_frequency_must_divide_the_ultrasound_frequency() {
        assert_eq!(
            SamplingConfig::from_freq(40000. * Hz),
            Ok(SamplingConfig::FREQ_40K)
        );
        assert_eq!(
            SamplingConfig::from_freq(4000. * Hz),
            Ok(SamplingConfig::FREQ_4K)
        );
        assert_eq!(
            SamplingConfig::from_freq(4000 * Hz),
            Ok(SamplingConfig::FREQ_4K)
        );
        assert_eq!(
            SamplingConfig::from_freq(40 * kHz),
            Ok(SamplingConfig::FREQ_40K)
        );
        let off = (ULTRASOUND_FREQ_HZ as f32 - 1.) * Hz;
        assert_eq!(
            SamplingConfig::from_freq(off),
            Err(SamplingConfigError::FreqInvalid(off))
        );
        assert_eq!(
            SamplingConfig::from_freq(39999 * Hz),
            Err(SamplingConfigError::FreqInvalid(39999. * Hz))
        );
        assert!(matches!(
            SamplingConfig::from_freq(80000. * Hz),
            Err(SamplingConfigError::FreqOutOfRange(..))
        ));
        assert!(SamplingConfig::from_freq(f32::NAN * Hz).is_err());
    }

    #[test]
    fn a_period_must_be_a_multiple_of_the_ultrasound_period() {
        assert_eq!(
            SamplingConfig::from_period(Duration::from_micros(25)),
            Ok(SamplingConfig::FREQ_40K)
        );
        assert_eq!(
            SamplingConfig::from_period(Duration::from_micros(250)),
            Ok(SamplingConfig::FREQ_4K)
        );
        assert_eq!(
            SamplingConfig::from_period(ULTRASOUND_PERIOD / 2),
            Err(SamplingConfigError::PeriodOutOfRange(
                ULTRASOUND_PERIOD / 2,
                ULTRASOUND_PERIOD,
                PERIOD_MAX
            ))
        );
        assert_eq!(
            SamplingConfig::from_period(Duration::from_nanos(50_001)),
            Err(SamplingConfigError::PeriodInvalid(Duration::from_nanos(
                50_001
            )))
        );
    }

    #[test]
    fn freq_and_period() {
        assert_eq!(40000. * Hz, SamplingConfig::FREQ_40K.freq());
        assert_eq!(4000. * Hz, SamplingConfig::FREQ_4K.freq());
        assert_eq!(Duration::from_micros(25), SamplingConfig::FREQ_40K.period());
        assert_eq!(Duration::from_micros(250), SamplingConfig::FREQ_4K.period());
    }

    #[test]
    fn err_display() {
        assert_eq!(
            "Sampling frequency (39999 Hz) must divide the ultrasound frequency",
            format!("{}", SamplingConfigError::FreqInvalid(39999. * Hz))
        );
    }
}

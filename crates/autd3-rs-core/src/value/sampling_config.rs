#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use core::{fmt::Debug, num::NonZeroU16, time::Duration};

use crate::common::{Freq, ULTRASOUND_FREQ, ULTRASOUND_PERIOD, units::Hz};

pub const IS_INTEGER_EPSILON: f64 = 1e-6;

#[must_use]
pub const fn is_integer(a: f64) -> bool {
    let dist = 0.5 - (a.fract() - 0.5).abs();
    dist < IS_INTEGER_EPSILON + a.abs() * (f64::EPSILON * 16.0)
}

#[derive(Debug, PartialEq, Copy, Clone, thiserror::Error)]
pub enum SamplingConfigError {
    #[error("Sampling frequency ({0:?}) must divide the ultrasound frequency")]
    FreqInvalid(Freq<u32>),
    #[error("Sampling frequency ({0:?}) must divide the ultrasound frequency")]
    FreqInvalidF(Freq<f32>),
    #[error("Sampling period ({0:?}) must be a multiple of the ultrasound period")]
    PeriodInvalid(Duration),
    #[error("Sampling frequency ({0:?}) is out of range ([{1:?}, {2:?}])")]
    FreqOutOfRange(Freq<u32>, Freq<u32>, Freq<u32>),
    #[error("Sampling frequency ({0:?}) is out of range ([{1:?}, {2:?}])")]
    FreqOutOfRangeF(Freq<f32>, Freq<f32>, Freq<f32>),
    #[error("Sampling period ({0:?}) is out of range ([{1:?}, {2:?}])")]
    PeriodOutOfRange(Duration, Duration, Duration),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Nearest<T: Copy + Clone + Debug + PartialEq>(pub T);

#[derive(Clone, Copy)]
pub enum SamplingConfig {
    Divide(NonZeroU16),
    Freq(Freq<f32>),
    Period(Duration),
    FreqNearest(Nearest<Freq<f32>>),
    PeriodNearest(Nearest<Duration>),
}

impl PartialEq for SamplingConfig {
    fn eq(&self, other: &Self) -> bool {
        match (self.divide(), other.divide()) {
            (Ok(lhs), Ok(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

impl Debug for SamplingConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SamplingConfig::Divide(div) => write!(f, "SamplingConfig::Divide({div})"),
            SamplingConfig::Freq(freq) => write!(f, "SamplingConfig::Freq({freq:?})"),
            SamplingConfig::Period(period) => write!(f, "SamplingConfig::Period({period:?})"),
            SamplingConfig::FreqNearest(nearest) => {
                write!(f, "SamplingConfig::FreqNearest({nearest:?})")
            }
            SamplingConfig::PeriodNearest(nearest) => {
                write!(f, "SamplingConfig::PeriodNearest({nearest:?})")
            }
        }
    }
}

impl From<NonZeroU16> for SamplingConfig {
    fn from(value: NonZeroU16) -> Self {
        Self::Divide(value)
    }
}

impl From<Freq<f32>> for SamplingConfig {
    fn from(value: Freq<f32>) -> Self {
        Self::Freq(value)
    }
}

impl From<Duration> for SamplingConfig {
    fn from(value: Duration) -> Self {
        Self::Period(value)
    }
}

impl SamplingConfig {
    pub const FREQ_40K: Self = SamplingConfig::Freq(Freq { freq: 40000. });
    pub const FREQ_4K: Self = SamplingConfig::Freq(Freq { freq: 4000. });

    #[must_use]
    pub fn new(value: impl Into<SamplingConfig>) -> Self {
        value.into()
    }

    pub fn divide(&self) -> Result<u16, SamplingConfigError> {
        match *self {
            SamplingConfig::Divide(div) => Ok(div.get()),
            SamplingConfig::Freq(freq) => {
                let freq_max = ULTRASOUND_FREQ.hz() as f32 * Hz;
                let freq_min = freq_max / u16::MAX as f32;
                if !(freq_min..=freq_max).contains(&freq) {
                    return Err(SamplingConfigError::FreqOutOfRangeF(
                        freq, freq_min, freq_max,
                    ));
                }
                let divide = ULTRASOUND_FREQ.hz() as f32 / freq.hz();
                if !is_integer(divide as f64) {
                    return Err(SamplingConfigError::FreqInvalidF(freq));
                }
                Ok(divide as u16)
            }
            SamplingConfig::Period(duration) => {
                let period_min = ULTRASOUND_PERIOD;
                let period_max =
                    Duration::from_micros(u16::MAX as u64 * ULTRASOUND_PERIOD.as_micros() as u64);
                if !(period_min..=period_max).contains(&duration) {
                    return Err(SamplingConfigError::PeriodOutOfRange(
                        duration, period_min, period_max,
                    ));
                }
                if duration.as_nanos() % ULTRASOUND_PERIOD.as_nanos() != 0 {
                    return Err(SamplingConfigError::PeriodInvalid(duration));
                }
                Ok((duration.as_nanos() / ULTRASOUND_PERIOD.as_nanos()) as u16)
            }
            SamplingConfig::FreqNearest(nearest) => Ok(((ULTRASOUND_FREQ.hz() as f32
                / nearest.0.hz())
            .clamp(1.0, u16::MAX as f32))
            .round() as u16),
            SamplingConfig::PeriodNearest(nearest) => {
                Ok(((nearest.0.as_nanos() + ULTRASOUND_PERIOD.as_nanos() / 2)
                    / ULTRASOUND_PERIOD.as_nanos())
                .clamp(1, u16::MAX as u128) as u16)
            }
        }
    }

    pub fn freq(&self) -> Result<Freq<f32>, SamplingConfigError> {
        Ok(ULTRASOUND_FREQ.hz() as f32 / self.divide()? as f32 * Hz)
    }

    pub fn period(&self) -> Result<Duration, SamplingConfigError> {
        Ok(ULTRASOUND_PERIOD * u32::from(self.divide()?))
    }

    #[must_use]
    pub const fn into_nearest(self) -> SamplingConfig {
        match self {
            SamplingConfig::Freq(freq) => SamplingConfig::FreqNearest(Nearest(freq)),
            SamplingConfig::Period(period) => SamplingConfig::PeriodNearest(Nearest(period)),
            _ => self,
        }
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
    fn divide() {
        let max_period =
            Duration::from_micros(u16::MAX as u64 * ULTRASOUND_PERIOD.as_micros() as u64);
        let cases: [(Result<u16, SamplingConfigError>, SamplingConfig); 8] = [
            (Ok(1), SamplingConfig::Divide(NonZeroU16::MIN)),
            (Ok(u16::MAX), SamplingConfig::Divide(NonZeroU16::MAX)),
            (Ok(1), SamplingConfig::new(40000. * Hz)),
            (Ok(10), SamplingConfig::new(4000. * Hz)),
            (
                Err(SamplingConfigError::FreqInvalidF(
                    (ULTRASOUND_FREQ.hz() as f32 - 1.) * Hz,
                )),
                SamplingConfig::new((ULTRASOUND_FREQ.hz() as f32 - 1.) * Hz),
            ),
            (Ok(1), SamplingConfig::new(Duration::from_micros(25))),
            (Ok(10), SamplingConfig::new(Duration::from_micros(250))),
            (
                Err(SamplingConfigError::PeriodOutOfRange(
                    ULTRASOUND_PERIOD / 2,
                    ULTRASOUND_PERIOD,
                    max_period,
                )),
                SamplingConfig::new(ULTRASOUND_PERIOD / 2),
            ),
        ];
        for (expect, config) in cases {
            assert_eq!(expect, config.divide());
        }
    }

    #[test]
    fn freq_and_period() {
        assert_eq!(
            Ok(40000. * Hz),
            SamplingConfig::Divide(NonZeroU16::MIN).freq()
        );
        assert_eq!(Ok(4000. * Hz), SamplingConfig::new(4000. * Hz).freq());
        assert_eq!(
            Ok(Duration::from_micros(25)),
            SamplingConfig::Divide(NonZeroU16::MIN).period()
        );
        assert_eq!(
            Ok(Duration::from_micros(250)),
            SamplingConfig::new(4000. * Hz).period()
        );
    }

    #[test]
    fn into_nearest_divide() {
        assert_eq!(
            Ok(1),
            SamplingConfig::new(40000. * Hz).into_nearest().divide()
        );
        assert_eq!(
            Ok(u16::MAX),
            SamplingConfig::new(0. * Hz).into_nearest().divide()
        );
        assert_eq!(
            Ok(1),
            SamplingConfig::new(ULTRASOUND_PERIOD / 2)
                .into_nearest()
                .divide()
        );
    }

    #[test]
    fn partial_eq() {
        assert!(SamplingConfig::FREQ_40K == SamplingConfig::new(NonZeroU16::MIN));
        assert!(SamplingConfig::FREQ_40K == SamplingConfig::new(40. * kHz));
        assert!(SamplingConfig::FREQ_40K == SamplingConfig::new(Duration::from_micros(25)));
        assert!(SamplingConfig::new(41. * kHz) != SamplingConfig::new(41. * kHz));
    }

    #[test]
    fn debug() {
        assert_eq!(
            "SamplingConfig::Divide(1)",
            format!("{:?}", SamplingConfig::Divide(NonZeroU16::MIN))
        );
        assert_eq!(
            "SamplingConfig::Freq(1 Hz)",
            format!("{:?}", SamplingConfig::Freq(1. * Hz))
        );
    }

    #[test]
    fn err_display() {
        assert_eq!(
            "Sampling frequency (39999 Hz) must divide the ultrasound frequency",
            format!("{}", SamplingConfigError::FreqInvalid(39999 * Hz))
        );
    }
}

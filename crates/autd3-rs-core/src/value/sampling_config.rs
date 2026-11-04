#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use core::{num::NonZeroU16, time::Duration};

use crate::common::{Freq, ULTRASOUND_PERIOD, units::Hz};
use crate::params::ULTRASOUND_FREQ_HZ;

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
        let divide = ULTRASOUND_FREQ_HZ as f64 / freq.hz() as f64;
        let nearest = divide.round();
        if (divide - nearest).abs() > nearest * f32::EPSILON as f64 * 4.0 {
            return Err(SamplingConfigError::FreqInvalid(freq));
        }
        Ok(Self(at_least_one(nearest as u16)))
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
    use rstest::rstest;

    use super::*;
    use crate::common::units::kHz;

    #[rstest]
    #[case(1, SamplingConfig::new(NonZeroU16::MIN))]
    #[case(u16::MAX, SamplingConfig::new(NonZeroU16::MAX))]
    #[case(10, SamplingConfig::FREQ_4K)]
    fn a_divider_is_taken_as_is(#[case] expected: u16, #[case] config: SamplingConfig) {
        assert_eq!(expected, config.divide().get());
    }

    #[rstest]
    #[case(SamplingConfig::FREQ_40K, 40000. * Hz)]
    #[case(SamplingConfig::FREQ_4K, 4000. * Hz)]
    #[case(SamplingConfig::FREQ_4K, 4000 * Hz)]
    #[case(SamplingConfig::FREQ_40K, 40 * kHz)]
    fn a_frequency_dividing_the_ultrasound_frequency_is_accepted(
        #[case] expected: SamplingConfig,
        #[case] freq: Freq,
    ) {
        assert_eq!(Ok(expected), SamplingConfig::from_freq(freq));
    }

    #[rstest]
    #[case((ULTRASOUND_FREQ_HZ as f32 - 1.) * Hz)]
    #[case(39999 * Hz)]
    fn a_frequency_not_dividing_the_ultrasound_frequency_is_rejected(#[case] freq: Freq) {
        assert_eq!(
            Err(SamplingConfigError::FreqInvalid(freq)),
            SamplingConfig::from_freq(freq)
        );
    }

    #[rstest]
    #[case(80000.)]
    #[case(-40000.)]
    #[case(-4000.)]
    #[case(-0.)]
    #[case(0.)]
    #[case(f32::NEG_INFINITY)]
    #[case(f32::INFINITY)]
    #[case(f32::NAN)]
    fn a_frequency_out_of_range_is_rejected(#[case] hz: f32) {
        assert!(matches!(
            SamplingConfig::from_freq(hz * Hz),
            Err(SamplingConfigError::FreqOutOfRange(..))
        ));
    }

    #[rstest]
    #[case(Ok(6), 6666.667)]
    #[case(Ok(7), 5714.286)]
    #[case(Err(SamplingConfigError::FreqInvalid(6666.7 * Hz)), 6666.7)]
    fn a_frequency_near_a_divider_takes_that_divider(
        #[case] expected: Result<u16, SamplingConfigError>,
        #[case] hz: f32,
    ) {
        assert_eq!(
            expected,
            SamplingConfig::from_freq(hz * Hz).map(|c| c.divide().get())
        );
    }

    #[test]
    fn every_divider_survives_the_round_trip_through_its_frequency() {
        (1..=u16::MAX).for_each(|d| {
            let config = SamplingConfig::new(NonZeroU16::new(d).unwrap());
            assert_eq!(SamplingConfig::from_freq(config.freq()), Ok(config));
        });
    }

    #[test]
    fn a_frequency_between_two_dividers_is_rejected() {
        (1..u16::MAX).for_each(|d| {
            let between = (ULTRASOUND_FREQ_HZ as f64 / (d as f64 + 0.5)) as f32 * Hz;
            assert_eq!(
                SamplingConfig::from_freq(between),
                Err(SamplingConfigError::FreqInvalid(between))
            );
        });
    }

    #[rstest]
    #[case(Ok(SamplingConfig::FREQ_40K), Duration::from_micros(25))]
    #[case(Ok(SamplingConfig::FREQ_4K), Duration::from_micros(250))]
    #[case(
        Err(SamplingConfigError::PeriodOutOfRange(
            ULTRASOUND_PERIOD / 2,
            ULTRASOUND_PERIOD,
            PERIOD_MAX
        )),
        ULTRASOUND_PERIOD / 2
    )]
    #[case(
        Err(SamplingConfigError::PeriodInvalid(Duration::from_nanos(50_001))),
        Duration::from_nanos(50_001)
    )]
    fn a_period_must_be_a_multiple_of_the_ultrasound_period(
        #[case] expected: Result<SamplingConfig, SamplingConfigError>,
        #[case] period: Duration,
    ) {
        assert_eq!(expected, SamplingConfig::from_period(period));
    }

    #[rstest]
    #[case(40000. * Hz, Duration::from_micros(25), SamplingConfig::FREQ_40K)]
    #[case(4000. * Hz, Duration::from_micros(250), SamplingConfig::FREQ_4K)]
    fn freq_and_period(
        #[case] freq: Freq,
        #[case] period: Duration,
        #[case] config: SamplingConfig,
    ) {
        assert_eq!(freq, config.freq());
        assert_eq!(period, config.period());
    }

    #[test]
    fn err_display() {
        assert_eq!(
            "Sampling frequency (39999 Hz) must divide the ultrasound frequency",
            format!("{}", SamplingConfigError::FreqInvalid(39999. * Hz))
        );
    }
}

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use core::num::NonZeroU16;

use autd3_rs_core::Freq;
use autd3_rs_core::params::{MOD_BUFFER_SAMPLES, ULTRASOUND_FREQ_HZ};
use autd3_rs_core::value::SamplingConfig;

use crate::error::ModulationError;

#[must_use]
const fn is_integer(a: f64) -> bool {
    let dist = 0.5 - (a.fract() - 0.5).abs();
    dist < 1e-6 + a.abs() * (f64::EPSILON * 16.0)
}

#[must_use]
pub fn samples_per_period(divider: NonZeroU16, freq: Freq) -> Option<u32> {
    let divider = u32::from(divider.get());
    if !ULTRASOUND_FREQ_HZ.is_multiple_of(divider) {
        return None;
    }
    let samples = f64::from(ULTRASOUND_FREQ_HZ / divider) / f64::from(freq.hz());
    (samples.is_finite() && (1.0..=f64::from(u32::MAX)).contains(&samples) && is_integer(samples))
        .then(|| samples.round() as u32)
}

pub(crate) fn samples_and_cycles(
    freq: Freq,
    config: SamplingConfig,
) -> Result<(usize, u64), ModulationError> {
    if freq.hz() < 0. || freq.hz().is_nan() {
        return Err(ModulationError::FrequencyNotPositive { hz: freq.hz() });
    }
    if freq.hz() == 0. {
        return Err(ModulationError::FrequencyZero);
    }
    let nyquist = config.freq().hz() / 2.;
    if freq.hz() >= nyquist {
        return Err(ModulationError::FrequencyAboveNyquist {
            hz: freq.hz(),
            nyquist,
        });
    }
    let fd = f64::from(freq.hz()) * f64::from(config.divide().get());
    let fs = u64::from(ULTRASOUND_FREQ_HZ);
    ((f64::from(ULTRASOUND_FREQ_HZ) / fd).floor() as u32..=MOD_BUFFER_SAMPLES as u32)
        .find_map(|n| {
            if !is_integer(fd * f64::from(n)) {
                return None;
            }
            let fnd = (fd * f64::from(n)) as u64;
            if !fnd.is_multiple_of(fs) {
                return None;
            }
            Some((n as usize, fnd / fs))
        })
        .ok_or(ModulationError::FrequencyNotRepresentable { hz: freq.hz() })
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(true, 0.0)]
    #[case(true, 42.0)]
    #[case(true, 1e12)]
    #[case(false, 2.5)]
    #[case(false, 1e12 + 0.5)]
    #[case(true, 1e12 + 1e-4)]
    fn is_integer_uses_relative_tolerance(#[case] expected: bool, #[case] a: f64) {
        assert_eq!(expected, is_integer(a));
    }

    #[rstest]
    #[case(Some(20), 10, 200 * Hz)]
    #[case(Some(200), 1, 200 * Hz)]
    #[case(Some(8000), 10, 0.5 * Hz)]
    #[case(None, 10, 300 * Hz)]
    #[case(None, 10, 0.1 * Hz)]
    #[case(None, 10, 0 * Hz)]
    #[case(None, 10, -200.0 * Hz)]
    #[case(None, 10, f32::NAN * Hz)]
    #[case(None, 3, 100 * Hz)]
    fn samples_per_period_divides_sampling_rate(
        #[case] expected: Option<u32>,
        #[case] divider: u16,
        #[case] freq: Freq,
    ) {
        assert_eq!(
            expected,
            samples_per_period(NonZeroU16::new(divider).unwrap(), freq)
        );
    }
}

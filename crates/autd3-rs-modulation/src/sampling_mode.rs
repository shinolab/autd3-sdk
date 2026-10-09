#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use autd3_rs_core::common::Freq;
use autd3_rs_core::params::{MOD_BUFFER_SAMPLES, ULTRASOUND_FREQ_HZ};
use autd3_rs_core::value::{Nearest, SamplingConfig, is_integer};

use crate::error::ModulationError;

pub(crate) fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum SamplingMode {
    ExactFreq(Freq<u32>),
    ExactFreqFloat(Freq<f32>),
    NearestFreq(Freq<f32>),
}

impl From<Freq<u32>> for SamplingMode {
    fn from(v: Freq<u32>) -> Self {
        SamplingMode::ExactFreq(v)
    }
}

impl From<Freq<f32>> for SamplingMode {
    fn from(v: Freq<f32>) -> Self {
        SamplingMode::ExactFreqFloat(v)
    }
}

impl From<Nearest<Freq<f32>>> for SamplingMode {
    fn from(v: Nearest<Freq<f32>>) -> Self {
        SamplingMode::NearestFreq(v.0)
    }
}

impl From<Nearest<Freq<u32>>> for SamplingMode {
    fn from(v: Nearest<Freq<u32>>) -> Self {
        SamplingMode::NearestFreq(Freq::from_hz(v.0.hz() as f32))
    }
}

impl SamplingMode {
    pub(crate) fn validate(self, config: SamplingConfig) -> Result<(usize, u64), ModulationError> {
        match self {
            SamplingMode::ExactFreq(freq) => Self::validate_exact(freq, config),
            SamplingMode::ExactFreqFloat(freq) => Self::validate_exact_f(freq, config),
            SamplingMode::NearestFreq(freq) => Self::validate_nearest(freq, config),
        }
    }

    fn validate_exact(
        freq: Freq<u32>,
        config: SamplingConfig,
    ) -> Result<(usize, u64), ModulationError> {
        let nyquist = config.freq()?.hz() / 2.;
        if freq.hz() as f32 >= nyquist {
            return Err(ModulationError::FrequencyAboveNyquist {
                hz: freq.hz() as f32,
                nyquist,
            });
        }
        if freq.hz() == 0 {
            return Err(ModulationError::FrequencyZero);
        }
        let fd = u64::from(freq.hz()) * u64::from(config.divide()?.get());
        let fs = u64::from(ULTRASOUND_FREQ_HZ);
        let k = gcd(fs, fd);
        Ok(((fs / k) as usize, fd / k))
    }

    fn validate_exact_f(
        freq: Freq<f32>,
        config: SamplingConfig,
    ) -> Result<(usize, u64), ModulationError> {
        if freq.hz() < 0. || freq.hz().is_nan() {
            return Err(ModulationError::FrequencyNotPositive { hz: freq.hz() });
        }
        if freq.hz() == 0. {
            return Err(ModulationError::FrequencyZero);
        }
        let nyquist = config.freq()?.hz() / 2.;
        if freq.hz() >= nyquist {
            return Err(ModulationError::FrequencyAboveNyquist {
                hz: freq.hz(),
                nyquist,
            });
        }
        let fd = f64::from(freq.hz()) * f64::from(config.divide()?.get());
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

    fn validate_nearest(
        freq: Freq<f32>,
        config: SamplingConfig,
    ) -> Result<(usize, u64), ModulationError> {
        let cfg_freq = config.freq()?.hz();
        let freq_min = cfg_freq / MOD_BUFFER_SAMPLES as f32;
        let freq_max = cfg_freq / 2.;
        let freq = freq.hz().clamp(freq_min, freq_max);
        if freq.is_nan() {
            return Err(ModulationError::FrequencyNaN);
        }
        Ok(((cfg_freq / freq).round() as usize, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_rs_core::units::Hz;

    #[test]
    fn a_nearest_integer_frequency_is_a_nearest_float_frequency() {
        assert_eq!(
            SamplingMode::from(Nearest(200 * Hz)),
            SamplingMode::from(Nearest(200.0 * Hz))
        );
    }
}

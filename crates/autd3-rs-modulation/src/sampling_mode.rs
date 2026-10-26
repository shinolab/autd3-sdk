#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use autd3_rs_core::common::Freq;
use autd3_rs_core::params::{MOD_BUFFER_SAMPLES, ULTRASOUND_FREQ_HZ};
use autd3_rs_core::value::{SamplingConfig, is_integer};

use crate::error::ModulationError;

#[derive(Clone, Debug, PartialEq)]
pub struct Nearest<T>(pub T);

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum SamplingMode {
    ExactFreqFloat(Freq),
    NearestFreq(Freq),
}

impl From<Freq> for SamplingMode {
    fn from(v: Freq) -> Self {
        SamplingMode::ExactFreqFloat(v)
    }
}

impl From<Nearest<Freq>> for SamplingMode {
    fn from(v: Nearest<Freq>) -> Self {
        SamplingMode::NearestFreq(v.0)
    }
}

impl SamplingMode {
    pub(crate) fn validate(self, config: SamplingConfig) -> Result<(usize, u64), ModulationError> {
        match self {
            SamplingMode::ExactFreqFloat(freq) => {
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
            SamplingMode::NearestFreq(freq) => {
                let cfg_freq = config.freq().hz();
                let freq_min = cfg_freq / MOD_BUFFER_SAMPLES as f32;
                let freq_max = cfg_freq / 2.;
                let freq = freq.hz().clamp(freq_min, freq_max);
                if freq.is_nan() {
                    return Err(ModulationError::FrequencyNaN);
                }
                Ok(((cfg_freq / freq).round() as usize, 1))
            }
        }
    }
}

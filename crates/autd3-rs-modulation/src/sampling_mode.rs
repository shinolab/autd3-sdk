#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use autd3_rs_core::common::{Freq, ULTRASOUND_FREQ};
use autd3_rs_core::error::Error;
use autd3_rs_core::params::MOD_BUFFER_SAMPLES;
use autd3_rs_core::value::SamplingConfig;

const IS_INTEGER_EPSILON: f64 = 1e-6;

fn is_integer(a: f64) -> bool {
    0.5 - (a.fract() - 0.5).abs() < IS_INTEGER_EPSILON
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn cfg_err(e: autd3_rs_core::value::SamplingConfigError) -> Error {
    Error::InvalidPayload(e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest(pub Freq<f32>);

#[derive(Clone, Copy, Debug, PartialEq)]
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

impl From<Nearest> for SamplingMode {
    fn from(v: Nearest) -> Self {
        SamplingMode::NearestFreq(v.0)
    }
}

impl SamplingMode {
    // Returns `(n, rep)`: `n` samples span `rep` full periods of the waveform.
    pub(crate) fn validate(self, config: SamplingConfig) -> Result<(u64, u64), Error> {
        match self {
            SamplingMode::ExactFreq(freq) => Self::validate_exact(freq, config),
            SamplingMode::ExactFreqFloat(freq) => Self::validate_exact_f(freq, config),
            SamplingMode::NearestFreq(freq) => Self::validate_nearest(freq, config),
        }
    }

    fn validate_exact(freq: Freq<u32>, config: SamplingConfig) -> Result<(u64, u64), Error> {
        let nyquist = config.freq().map_err(cfg_err)?.hz() / 2.;
        if freq.hz() as f32 >= nyquist {
            return Err(Error::InvalidPayload(format!(
                "frequency ({freq:?}) is equal to or greater than the Nyquist frequency ({nyquist} Hz)"
            )));
        }
        if freq.hz() == 0 {
            return Err(Error::InvalidPayload(
                "modulation frequency must not be zero".into(),
            ));
        }
        let fd = u64::from(freq.hz()) * u64::from(config.divide().map_err(cfg_err)?);
        let fs = u64::from(ULTRASOUND_FREQ.hz());
        let k = gcd(fs, fd);
        Ok((fs / k, fd / k))
    }

    fn validate_exact_f(freq: Freq<f32>, config: SamplingConfig) -> Result<(u64, u64), Error> {
        if freq.hz() < 0. || freq.hz().is_nan() {
            return Err(Error::InvalidPayload(format!(
                "frequency ({freq:?}) must be a valid positive value"
            )));
        }
        if freq.hz() == 0. {
            return Err(Error::InvalidPayload(
                "modulation frequency must not be zero".into(),
            ));
        }
        let nyquist = config.freq().map_err(cfg_err)?.hz() / 2.;
        if freq.hz() >= nyquist {
            return Err(Error::InvalidPayload(format!(
                "frequency ({freq:?}) is equal to or greater than the Nyquist frequency ({nyquist} Hz)"
            )));
        }
        let fd = f64::from(freq.hz()) * f64::from(config.divide().map_err(cfg_err)?);
        let fs = u64::from(ULTRASOUND_FREQ.hz());
        ((f64::from(ULTRASOUND_FREQ.hz()) / fd).floor() as u32..=MOD_BUFFER_SAMPLES as u32)
            .find_map(|n| {
                if !is_integer(fd * f64::from(n)) {
                    return None;
                }
                let fnd = (fd * f64::from(n)) as u64;
                if !fnd.is_multiple_of(fs) {
                    return None;
                }
                Some((u64::from(n), fnd / fs))
            })
            .ok_or_else(|| {
                Error::InvalidPayload(format!(
                    "frequency ({freq:?}) cannot be output with the sampling config ({config:?})"
                ))
            })
    }

    fn validate_nearest(freq: Freq<f32>, config: SamplingConfig) -> Result<(u64, u64), Error> {
        let cfg_freq = config.freq().map_err(cfg_err)?.hz();
        let freq_min = cfg_freq / MOD_BUFFER_SAMPLES as f32;
        let freq_max = cfg_freq / 2.;
        let freq = freq.hz().clamp(freq_min, freq_max);
        if freq.is_nan() {
            return Err(Error::InvalidPayload(
                "modulation frequency must be a valid value".into(),
            ));
        }
        Ok(((cfg_freq / freq).round() as u64, 1))
    }
}

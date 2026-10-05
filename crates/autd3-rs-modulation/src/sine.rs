#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use core::f32::consts::PI;

use autd3_rs_core::common::Angle;
use autd3_rs_core::value::SamplingConfig;

use crate::error::ModulationError;
use crate::quantize::quantize;
use crate::sampling_mode::SamplingMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SineOption {
    pub amplitude: u8,
    pub offset: u8,
    pub phase: Angle,
    pub clamp: bool,
    pub sampling_config: SamplingConfig,
}

impl Default for SineOption {
    fn default() -> Self {
        Self {
            amplitude: 0xFF,
            offset: 0x80,
            phase: Angle::ZERO,
            clamp: false,
            sampling_config: SamplingConfig::FREQ_4K,
        }
    }
}

pub(crate) fn sine_samples<S: Into<SamplingMode>>(
    freq: S,
    option: &SineOption,
) -> Result<impl ExactSizeIterator<Item = f32> + use<S>, ModulationError> {
    let mode: SamplingMode = freq.into();
    let (n, rep) = mode.validate(option.sampling_config)?;

    let amplitude = f32::from(option.amplitude);
    let offset = f32::from(option.offset);
    let phase = option.phase.rad();

    Ok((0..n).map(move |i| {
        let t = (rep * i as u64) as f32 / n as f32;
        (amplitude / 2.0 * (2.0 * PI * t + phase).sin()) + offset
    }))
}

pub fn sine<S: Into<SamplingMode>>(
    freq: S,
    option: &SineOption,
    dst: &mut Vec<u8>,
) -> Result<(), ModulationError> {
    if quantize(sine_samples(freq, option)?, option.clamp, dst) {
        return Err(ModulationError::SineValueOutOfRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;
    use autd3_rs_core::value::Nearest;

    use super::*;

    const SINE_200HZ_DEFAULT: &[u8] = &[
        128, 167, 202, 231, 249, 255, 249, 231, 202, 167, 127, 88, 53, 24, 6, 0, 6, 24, 53, 88,
    ];

    #[test]
    fn sine_200hz_default_matches_reference() {
        let mut buf = Vec::new();
        sine(200 * Hz, &SineOption::default(), &mut buf).unwrap();
        assert_eq!(buf.as_slice(), SINE_200HZ_DEFAULT);
    }

    #[test]
    fn sine_nearest_frequency_matches_exact() {
        let mut buf = Vec::new();
        sine(Nearest(200.0 * Hz), &SineOption::default(), &mut buf).unwrap();
        assert_eq!(buf.as_slice(), SINE_200HZ_DEFAULT);
    }

    #[test]
    fn sine_nearest_rounds_to_integer_sample_count() {
        let mut buf = Vec::new();
        sine(Nearest(190.0 * Hz), &SineOption::default(), &mut buf).unwrap();
        assert_eq!(buf.len(), 21);
        assert_eq!(buf[0], 0x80);
    }

    #[test]
    fn sine_float_frequency() {
        let mut buf = Vec::new();
        sine(200.0 * Hz, &SineOption::default(), &mut buf).unwrap();
        assert_eq!(buf.len(), 20);
        assert_eq!(buf.as_slice()[0], 128);
    }

    #[test]
    fn sine_zero_frequency_errors() {
        let mut buf = Vec::new();
        assert!(sine(0 * Hz, &SineOption::default(), &mut buf).is_err());
    }

    #[test]
    fn sine_out_of_range_errors_unless_clamped() {
        let mut buf = Vec::new();
        let opt = SineOption {
            offset: 0x00,
            ..Default::default()
        };
        assert!(sine(200 * Hz, &opt, &mut buf).is_err());

        let opt = SineOption {
            offset: 0x00,
            clamp: true,
            ..Default::default()
        };
        sine(200 * Hz, &opt, &mut buf).unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                0, 39, 74, 103, 121, 127, 121, 103, 74, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn sine_out_of_range_error_leaves_zero_at_the_offending_samples() {
        let mut buf = vec![1, 2, 3];
        let opt = SineOption {
            offset: 0xFF,
            ..Default::default()
        };
        assert_eq!(
            sine(200 * Hz, &opt, &mut buf),
            Err(ModulationError::SineValueOutOfRange)
        );
        assert_eq!(
            buf.as_slice(),
            &[
                255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 254, 215, 180, 151, 133, 127, 133, 151, 180, 215
            ]
        );
    }
}

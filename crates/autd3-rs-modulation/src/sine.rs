#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use core::f32::consts::PI;

use autd3_rs_core::common::Angle;
use autd3_rs_core::error::Error;
use autd3_rs_core::value::{Intensity, SamplingConfig};

use crate::sampling_mode::SamplingMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SineOption {
    pub intensity: Intensity,
    pub offset: u8,
    pub phase: Angle,
    pub clamp: bool,
    pub sampling_config: SamplingConfig,
}

impl Default for SineOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            offset: 0x80,
            phase: Angle::ZERO,
            clamp: false,
            sampling_config: SamplingConfig::FREQ_4K,
        }
    }
}

pub fn sine<S: Into<SamplingMode>>(
    freq: S,
    option: &SineOption,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    let mode: SamplingMode = freq.into();
    let (n, rep) = mode.validate(option.sampling_config)?;
    let n = usize::try_from(n)
        .map_err(|_| Error::InvalidPayload("modulation sample count exceeds usize".into()))?;

    let intensity = f32::from(option.intensity.0);
    let offset = f32::from(option.offset);
    let phase = option.phase.radian();

    out.clear();
    out.reserve(n);
    let mut out_of_range = false;
    for i in 0..n {
        let t = (rep * i as u64) as f32 / n as f32;
        let v = (intensity / 2.0 * (2.0 * PI * t + phase).sin()) + offset;
        let v = v.floor() as i16;
        out.push(if (0..=255).contains(&v) {
            v as u8
        } else if option.clamp {
            v.clamp(0, 255) as u8
        } else {
            out_of_range = true;
            0
        });
    }
    if out_of_range {
        return Err(Error::InvalidPayload(
            "sine modulation value is out of range [0, 255]".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;

    use super::*;

    #[test]
    fn sine_matches_legacy_vectors() {
        let mut buf = Vec::new();
        sine(200 * Hz, &SineOption::default(), &mut buf).unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                128, 167, 202, 231, 249, 255, 249, 231, 202, 167, 127, 88, 53, 24, 6, 0, 6, 24, 53,
                88
            ]
        );
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
}

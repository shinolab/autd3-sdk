#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use autd3_rs_core::common::Freq;
use autd3_rs_core::params::MOD_BUFFER_SAMPLES;
use autd3_rs_core::value::SamplingConfig;

use crate::error::ModulationError;
use crate::quantize::quantize;
use crate::sine::{SineOption, sine_samples};

use num_integer::Integer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SineComponent {
    pub freq: Freq,
    pub option: SineOption,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FourierOption {
    pub scale_factor: Option<f32>,
    pub clamp: bool,
    pub offset: u8,
}

pub fn fourier(
    components: &[SineComponent],
    option: &FourierOption,
    config: SamplingConfig,
    dst: &mut Vec<u8>,
) -> Result<(), ModulationError> {
    if components.is_empty() {
        return Err(ModulationError::FourierComponentsEmpty);
    }

    let buffers = components
        .iter()
        .map(|c| sine_samples(c.freq, c.option, config).map(Iterator::collect::<Vec<f32>>))
        .collect::<Result<Vec<_>, ModulationError>>()?;

    let scale = option.scale_factor.unwrap_or(1.0 / buffers.len() as f32);
    let offset = f32::from(option.offset);

    let len = buffers
        .iter()
        .try_fold(1usize, |acc, b| {
            acc.checked_mul(b.len() / acc.gcd(&b.len()))
                .filter(|&len| len <= MOD_BUFFER_SAMPLES)
        })
        .ok_or(ModulationError::FourierPeriodTooLong {
            max: MOD_BUFFER_SAMPLES,
        })?;
    let mut acc = vec![0f32; len];
    for buf in &buffers {
        for (a, b) in acc.iter_mut().zip(buf.iter().cycle()) {
            *a += *b;
        }
    }

    let samples = acc.into_iter().map(|v| v * scale + offset);
    quantize(samples, option.clamp, dst)
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;

    use super::*;

    #[test]
    fn fourier_single_component_matches_sine() {
        let mut buf = Vec::new();
        fourier(
            &[SineComponent {
                freq: 200 * Hz,
                option: SineOption {
                    offset: 0x00,
                    ..Default::default()
                },
            }],
            &FourierOption {
                clamp: true,
                ..Default::default()
            },
            SamplingConfig::FREQ_4K,
            &mut buf,
        )
        .unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                0, 39, 74, 103, 121, 127, 121, 103, 74, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn fourier_sum_matches_legacy_formula() {
        let components = [
            SineComponent {
                freq: 100 * Hz,
                option: SineOption::default(),
            },
            SineComponent {
                freq: 150 * Hz,
                option: SineOption::default(),
            },
            SineComponent {
                freq: 200 * Hz,
                option: SineOption::default(),
            },
        ];
        let mut buf = Vec::new();
        fourier(
            &components,
            &FourierOption::default(),
            SamplingConfig::FREQ_4K,
            &mut buf,
        )
        .unwrap();

        let raws = components
            .iter()
            .map(|c| {
                sine_samples(c.freq, c.option, SamplingConfig::FREQ_4K)
                    .unwrap()
                    .collect::<Vec<f32>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            buf.len(),
            raws.iter().fold(1, |acc, b| num_integer::lcm(acc, b.len()))
        );
        for (i, &v) in buf.iter().enumerate() {
            let sum: f32 = raws.iter().map(|b| b[i % b.len()]).sum();
            assert_eq!(v, (sum / 3.0).floor() as u8);
        }
    }

    fn components_with_periods(periods: &[f32]) -> Vec<SineComponent> {
        periods
            .iter()
            .map(|&period| SineComponent {
                freq: 4000.0 / period * Hz,
                option: SineOption::default(),
            })
            .collect()
    }

    #[test]
    fn fourier_accepts_a_period_up_to_the_buffer_and_rejects_one_past_it() {
        let mut buf = Vec::new();

        let fits = components_with_periods(&[256.0, 65536.0]);
        fourier(
            &fits,
            &FourierOption::default(),
            SamplingConfig::FREQ_4K,
            &mut buf,
        )
        .unwrap();
        assert_eq!(buf.len(), MOD_BUFFER_SAMPLES);

        let too_long = components_with_periods(&[5.0, 65536.0]);
        assert_eq!(
            fourier(
                &too_long,
                &FourierOption::default(),
                SamplingConfig::FREQ_4K,
                &mut buf
            ),
            Err(ModulationError::FourierPeriodTooLong {
                max: MOD_BUFFER_SAMPLES
            })
        );
    }

    #[test]
    fn fourier_applies_the_sampling_config_to_every_component() {
        let components = [
            SineComponent {
                freq: 100 * Hz,
                option: SineOption::default(),
            },
            SineComponent {
                freq: 160 * Hz,
                option: SineOption::default(),
            },
        ];
        let mut buf = Vec::new();
        fourier(
            &components,
            &FourierOption::default(),
            SamplingConfig::FREQ_40K,
            &mut buf,
        )
        .unwrap();
        assert_eq!(buf.len(), 2000);
    }

    #[test]
    fn fourier_empty_components_errors() {
        let mut buf = Vec::new();
        assert!(
            fourier(
                &[],
                &FourierOption::default(),
                SamplingConfig::FREQ_4K,
                &mut buf
            )
            .is_err()
        );
    }

    #[test]
    fn fourier_out_of_range_errors_unless_clamped() {
        let make = |offset: u8, clamp: bool, scale: Option<f32>, buf: &mut Vec<u8>| {
            fourier(
                &[SineComponent {
                    freq: 200 * Hz,
                    option: SineOption {
                        offset,
                        ..Default::default()
                    },
                }],
                &FourierOption {
                    clamp,
                    scale_factor: scale,
                    offset: 0,
                },
                SamplingConfig::FREQ_4K,
                buf,
            )
        };

        let mut buf = Vec::new();
        assert!(make(0x00, false, None, &mut buf).is_err());
        assert!(make(0xFF, false, Some(2.0), &mut buf).is_err());
        make(0x00, true, None, &mut buf).unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                0, 39, 74, 103, 121, 127, 121, 103, 74, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn fourier_out_of_range_error_leaves_zero_at_the_offending_samples() {
        let mut buf = vec![1, 2, 3];
        let result = fourier(
            &[SineComponent {
                freq: 200 * Hz,
                option: SineOption::default(),
            }],
            &FourierOption {
                scale_factor: Some(1.5),
                clamp: false,
                offset: 0,
            },
            SamplingConfig::FREQ_4K,
            &mut buf,
        );
        assert_eq!(result, Err(ModulationError::ValueOutOfRange));
        assert_eq!(
            buf.as_slice(),
            &[
                192, 251, 0, 0, 0, 0, 0, 0, 0, 251, 191, 132, 79, 37, 10, 0, 10, 37, 79, 132
            ]
        );
    }

    #[test]
    fn fourier_nan_scale_factor_is_reported() {
        let mut buf = Vec::new();
        let result = fourier(
            &[SineComponent {
                freq: 200 * Hz,
                option: SineOption::default(),
            }],
            &FourierOption {
                scale_factor: Some(f32::NAN),
                clamp: true,
                offset: 0,
            },
            SamplingConfig::FREQ_4K,
            &mut buf,
        );
        assert_eq!(result, Err(ModulationError::ValueNaN));
    }
}

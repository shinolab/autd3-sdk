#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use crate::error::ModulationError;
use crate::quantize::quantize;
use crate::sampling_mode::SamplingMode;
use crate::sine::{SineOption, sine_samples};

use num_integer::Integer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SineComponent {
    pub freq: SamplingMode,
    pub option: SineOption,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FourierOption {
    pub scale_factor: Option<f32>,
    pub clamp: bool,
    pub offset: u8,
}

pub fn fourier(
    components: &[SineComponent],
    option: &FourierOption,
    dst: &mut Vec<u8>,
) -> Result<(), ModulationError> {
    let Some(first) = components.first() else {
        return Err(ModulationError::FourierComponentsEmpty);
    };
    let sampling_config = first.option.sampling_config;
    if components
        .iter()
        .any(|c| c.option.sampling_config != sampling_config)
    {
        return Err(ModulationError::FourierSamplingConfigMismatch);
    }

    let buffers = components
        .iter()
        .map(|c| sine_samples(c.freq, &c.option).map(Iterator::collect::<Vec<f32>>))
        .collect::<Result<Vec<_>, ModulationError>>()?;

    let scale = option.scale_factor.unwrap_or(1.0 / buffers.len() as f32);
    let offset = f32::from(option.offset);

    let len = buffers.iter().fold(1usize, |acc, b| acc.lcm(&b.len()));
    let mut acc = vec![0f32; len];
    for buf in &buffers {
        for (a, b) in acc.iter_mut().zip(buf.iter().cycle()) {
            *a += *b;
        }
    }

    let samples = acc.into_iter().map(|v| v * scale + offset);
    if quantize(samples, option.clamp, dst) {
        return Err(ModulationError::FourierValueOutOfRange);
    }
    Ok(())
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
                freq: (200 * Hz).into(),
                option: SineOption {
                    offset: 0x00,
                    ..Default::default()
                },
            }],
            &FourierOption {
                clamp: true,
                ..Default::default()
            },
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
                freq: (100 * Hz).into(),
                option: SineOption::default(),
            },
            SineComponent {
                freq: (150 * Hz).into(),
                option: SineOption::default(),
            },
            SineComponent {
                freq: (200 * Hz).into(),
                option: SineOption::default(),
            },
        ];
        let mut buf = Vec::new();
        fourier(&components, &FourierOption::default(), &mut buf).unwrap();

        let raws = components
            .iter()
            .map(|c| {
                sine_samples(c.freq, &c.option)
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

    #[test]
    fn fourier_mixes_exact_and_nearest_components_in_one_slice() {
        use autd3_rs_core::value::Nearest;

        let mixed = [
            SineComponent {
                freq: (100 * Hz).into(),
                option: SineOption::default(),
            },
            SineComponent {
                freq: Nearest(200.0 * Hz).into(),
                option: SineOption::default(),
            },
        ];
        let exact = [
            SineComponent {
                freq: (100 * Hz).into(),
                option: SineOption::default(),
            },
            SineComponent {
                freq: (200 * Hz).into(),
                option: SineOption::default(),
            },
        ];
        let mut mixed_buf = Vec::new();
        let mut exact_buf = Vec::new();
        fourier(&mixed, &FourierOption::default(), &mut mixed_buf).unwrap();
        fourier(&exact, &FourierOption::default(), &mut exact_buf).unwrap();
        assert_eq!(mixed_buf, exact_buf);
    }

    #[test]
    fn fourier_empty_components_errors() {
        let mut buf = Vec::new();
        assert!(fourier(&[], &FourierOption::default(), &mut buf).is_err());
    }

    #[test]
    fn fourier_sampling_config_mismatch_errors() {
        use autd3_rs_core::value::SamplingConfig;

        let mut buf = Vec::new();
        let components = [
            SineComponent {
                freq: (50 * Hz).into(),
                option: SineOption {
                    sampling_config: SamplingConfig::FREQ_4K,
                    ..Default::default()
                },
            },
            SineComponent {
                freq: (50 * Hz).into(),
                option: SineOption {
                    sampling_config: SamplingConfig::FREQ_40K,
                    ..Default::default()
                },
            },
        ];
        assert!(fourier(&components, &FourierOption::default(), &mut buf).is_err());
    }

    #[test]
    fn fourier_out_of_range_errors_unless_clamped() {
        let make = |offset: u8, clamp: bool, scale: Option<f32>, buf: &mut Vec<u8>| {
            fourier(
                &[SineComponent {
                    freq: (200 * Hz).into(),
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
                freq: (200 * Hz).into(),
                option: SineOption::default(),
            }],
            &FourierOption {
                scale_factor: Some(1.5),
                clamp: false,
                offset: 0,
            },
            &mut buf,
        );
        assert_eq!(result, Err(ModulationError::FourierValueOutOfRange));
        assert_eq!(
            buf.as_slice(),
            &[
                192, 251, 0, 0, 0, 0, 0, 0, 0, 251, 191, 132, 79, 37, 10, 0, 10, 37, 79, 132
            ]
        );
    }
}

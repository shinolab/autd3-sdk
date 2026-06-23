#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use autd3_rs_core::error::{Error, PayloadError};

use crate::sampling_mode::SamplingMode;
use crate::sine::{SineOption, sine_raw};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SineComponent<S> {
    pub freq: S,
    pub option: SineOption,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FourierOption {
    pub scale_factor: Option<f32>,
    pub clamp: bool,
    pub offset: u8,
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn lcm(a: usize, b: usize) -> usize {
    a / gcd(a, b) * b
}

pub fn fourier<S: Into<SamplingMode> + Copy>(
    components: &[SineComponent<S>],
    option: &FourierOption,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    let Some(first) = components.first() else {
        return Err(Error::InvalidPayload(PayloadError::FourierComponentsEmpty));
    };
    let sampling_config = first.option.sampling_config;
    if components
        .iter()
        .any(|c| c.option.sampling_config != sampling_config)
    {
        return Err(Error::InvalidPayload(
            PayloadError::FourierSamplingConfigMismatch,
        ));
    }

    let buffers = components
        .iter()
        .map(|c| sine_raw(c.freq, &c.option))
        .collect::<Result<Vec<_>, Error>>()?;

    let scale = option.scale_factor.unwrap_or(1.0 / buffers.len() as f32);
    let offset = f32::from(option.offset);

    let len = buffers.iter().fold(1, |acc, b| lcm(acc, b.len()));
    let mut acc = vec![0f32; len];
    for buf in &buffers {
        for (a, b) in acc.iter_mut().zip(buf.iter().cycle()) {
            *a += *b;
        }
    }

    out.clear();
    out.reserve(len);
    let mut out_of_range = false;
    for v in acc {
        let v = (v * scale + offset).floor() as i32;
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
        return Err(Error::InvalidPayload(PayloadError::FourierValueOutOfRange));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::common::Freq;
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
        fourier(&components, &FourierOption::default(), &mut buf).unwrap();

        let raws = components
            .iter()
            .map(|c| sine_raw(c.freq, &c.option).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(buf.len(), raws.iter().fold(1, |acc, b| lcm(acc, b.len())));
        for (i, &v) in buf.iter().enumerate() {
            let sum: f32 = raws.iter().map(|b| b[i % b.len()]).sum();
            assert_eq!(v, (sum / 3.0).floor() as u8);
        }
    }

    #[test]
    fn fourier_empty_components_errors() {
        let mut buf = Vec::new();
        assert!(fourier::<Freq<u32>>(&[], &FourierOption::default(), &mut buf).is_err());
    }

    #[test]
    fn fourier_sampling_config_mismatch_errors() {
        use autd3_rs_core::value::SamplingConfig;

        let mut buf = Vec::new();
        let components = [
            SineComponent {
                freq: 50 * Hz,
                option: SineOption {
                    sampling_config: SamplingConfig::FREQ_4K,
                    ..Default::default()
                },
            },
            SineComponent {
                freq: 50 * Hz,
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
}

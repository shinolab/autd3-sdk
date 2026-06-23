#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use autd3_rs_core::error::{Error, PayloadError};
use autd3_rs_core::value::SamplingConfig;

use crate::sampling_mode::SamplingMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SquareOption {
    pub low: u8,
    pub high: u8,
    pub duty: f32,
    pub sampling_config: SamplingConfig,
}

impl Default for SquareOption {
    fn default() -> Self {
        Self {
            low: u8::MIN,
            high: u8::MAX,
            duty: 0.5,
            sampling_config: SamplingConfig::FREQ_4K,
        }
    }
}

pub fn square<S: Into<SamplingMode>>(
    freq: S,
    option: &SquareOption,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    if !(0.0..=1.0).contains(&option.duty) {
        return Err(Error::InvalidPayload(PayloadError::DutyOutOfRange {
            duty: option.duty,
        }));
    }

    let mode: SamplingMode = freq.into();
    let (n, rep) = mode.validate(option.sampling_config)?;
    let n =
        usize::try_from(n).map_err(|_| Error::InvalidPayload(PayloadError::SampleCountOverflow))?;

    out.clear();
    out.reserve(n);
    for i in 0..rep {
        let size = ((n as u64 + i) / rep) as usize;
        let n_high = (size as f32 * option.duty) as usize;
        out.extend(core::iter::repeat_n(option.high, n_high));
        out.extend(core::iter::repeat_n(option.low, size - n_high));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;

    use super::*;

    #[test]
    fn square_matches_legacy_vectors() {
        let mut buf = Vec::new();
        square(200 * Hz, &SquareOption::default(), &mut buf).unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn square_uneven_segments_match_legacy() {
        let mut buf = Vec::new();
        square(781.25 * Hz, &SquareOption::default(), &mut buf).unwrap();
        assert_eq!(
            buf.as_slice(),
            &[
                255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255,
                255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0,
                0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0,
                255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255,
                255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 0,
                0, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 0
            ]
        );
    }

    #[test]
    fn square_duty_extremes() {
        let mut buf = Vec::new();
        square(
            200 * Hz,
            &SquareOption {
                duty: 0.0,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        assert!(buf.iter().all(|&x| x == u8::MIN));

        square(
            200 * Hz,
            &SquareOption {
                duty: 1.0,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        assert!(buf.iter().all(|&x| x == u8::MAX));
    }

    #[test]
    fn square_low_high_swap() {
        let mut buf = Vec::new();
        square(
            150.0 * Hz,
            &SquareOption {
                low: u8::MAX,
                ..Default::default()
            },
            &mut buf,
        )
        .unwrap();
        assert!(buf.iter().all(|&x| x == u8::MAX));
    }

    #[test]
    fn square_duty_out_of_range_errors() {
        let mut buf = Vec::new();
        for duty in [-0.1, 1.1] {
            assert!(
                square(
                    150.0 * Hz,
                    &SquareOption {
                        duty,
                        ..Default::default()
                    },
                    &mut buf,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn square_zero_frequency_errors() {
        let mut buf = Vec::new();
        assert!(square(0 * Hz, &SquareOption::default(), &mut buf).is_err());
    }
}

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use crate::error::ModulationError;

pub(crate) fn quantize(
    samples: impl ExactSizeIterator<Item = f32>,
    clamp: bool,
    dst: &mut Vec<u8>,
) -> Result<(), ModulationError> {
    dst.clear();
    dst.reserve(samples.len());
    let mut nan = false;
    let mut out_of_range = false;
    dst.extend(samples.map(|v| {
        if v.is_nan() {
            nan = true;
            return 0;
        }
        let v = v.floor() as i32;
        if (0..=255).contains(&v) {
            v as u8
        } else if clamp {
            v.clamp(0, 255) as u8
        } else {
            out_of_range = true;
            0
        }
    }));
    if nan {
        Err(ModulationError::ValueNaN)
    } else if out_of_range {
        Err(ModulationError::ValueOutOfRange)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::in_range_samples_are_floored(Ok(()), &[0, 0, 254, 255], &[0.0, 0.9, 254.5, 255.9], false)]
    #[case::out_of_range_samples_become_zero_and_are_reported(
        Err(ModulationError::ValueOutOfRange),
        &[0, 0, 10],
        &[-0.1, 256.0, 10.0],
        false
    )]
    #[case::out_of_range_samples_saturate_when_clamped(
        Ok(()),
        &[0, 255, 255, 0],
        &[-0.1, 256.0, f32::INFINITY, f32::NEG_INFINITY],
        true
    )]
    #[case::infinite_samples_are_out_of_range_unless_clamped(
        Err(ModulationError::ValueOutOfRange),
        &[0, 0],
        &[f32::INFINITY, f32::NEG_INFINITY],
        false
    )]
    #[case::nan_samples_become_zero_and_are_reported(
        Err(ModulationError::ValueNaN),
        &[0, 1],
        &[f32::NAN, 1.0],
        false
    )]
    #[case::nan_samples_become_zero_and_are_reported_even_when_clamped(
        Err(ModulationError::ValueNaN),
        &[0, 1],
        &[f32::NAN, 1.0],
        true
    )]
    #[case::nan_takes_precedence_over_out_of_range(
        Err(ModulationError::ValueNaN),
        &[0, 0],
        &[256.0, f32::NAN],
        false
    )]
    fn quantize_floors_into_dst(
        #[case] expected: Result<(), ModulationError>,
        #[case] expected_dst: &[u8],
        #[case] samples: &[f32],
        #[case] clamp: bool,
    ) {
        let mut dst = vec![9];
        assert_eq!(expected, quantize(samples.iter().copied(), clamp, &mut dst));
        assert_eq!(expected_dst, dst.as_slice());
    }
}

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub(crate) fn quantize(
    samples: impl ExactSizeIterator<Item = f32>,
    clamp: bool,
    dst: &mut Vec<u8>,
) -> bool {
    dst.clear();
    dst.reserve(samples.len());
    let mut out_of_range = false;
    dst.extend(samples.map(|v| {
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
    out_of_range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_range_samples_are_floored() {
        let mut dst = vec![9];
        assert!(!quantize(
            [0.0, 0.9, 254.5, 255.9].into_iter(),
            false,
            &mut dst
        ));
        assert_eq!(dst.as_slice(), &[0, 0, 254, 255]);
    }

    #[test]
    fn out_of_range_samples_become_zero_and_are_reported() {
        let mut dst = Vec::new();
        assert!(quantize([-0.1, 256.0, 10.0].into_iter(), false, &mut dst));
        assert_eq!(dst.as_slice(), &[0, 0, 10]);
    }

    #[test]
    fn out_of_range_samples_saturate_when_clamped() {
        let mut dst = Vec::new();
        assert!(!quantize(
            [-0.1, 256.0, f32::INFINITY, f32::NEG_INFINITY].into_iter(),
            true,
            &mut dst
        ));
        assert_eq!(dst.as_slice(), &[0, 255, 255, 0]);
    }

    #[test]
    fn infinite_samples_are_out_of_range_unless_clamped() {
        let mut dst = Vec::new();
        assert!(quantize(
            [f32::INFINITY, f32::NEG_INFINITY].into_iter(),
            false,
            &mut dst
        ));
        assert_eq!(dst.as_slice(), &[0, 0]);
    }

    #[test]
    fn nan_samples_become_zero_without_being_reported() {
        let mut dst = Vec::new();
        assert!(!quantize([f32::NAN, 1.0].into_iter(), false, &mut dst));
        assert_eq!(dst.as_slice(), &[0, 1]);
    }
}

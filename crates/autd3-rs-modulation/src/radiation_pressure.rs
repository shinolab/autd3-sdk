#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub fn radiation_pressure(samples: &mut [u8]) {
    for v in samples.iter_mut() {
        *v = ((f32::from(*v) / 255.0).sqrt() * 255.0).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::matches_legacy_formula(&[0, 128, 181, 221, 255], vec![0, 64, 128, 191, 255])]
    #[case::empty(&[], vec![])]
    fn radiation_pressure_maps_each_sample(#[case] expected: &[u8], #[case] mut buf: Vec<u8>) {
        radiation_pressure(&mut buf);
        assert_eq!(expected, buf.as_slice());
    }
}

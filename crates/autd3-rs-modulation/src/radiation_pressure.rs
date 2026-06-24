#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub fn radiation_pressure(samples: &mut [u8]) {
    for v in samples.iter_mut() {
        *v = ((f32::from(*v) / 255.0).sqrt() * 255.0).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radiation_pressure_matches_legacy_formula() {
        let mut buf = [0, 64, 128, 191, 255];
        radiation_pressure(&mut buf);
        assert_eq!(buf, [0, 128, 181, 221, 255]);
    }

    #[test]
    fn radiation_pressure_empty() {
        let mut buf: [u8; 0] = [];
        radiation_pressure(&mut buf);
        assert_eq!(buf, []);
    }
}

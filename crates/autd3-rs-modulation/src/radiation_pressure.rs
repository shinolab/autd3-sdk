#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

fn apply(v: u8) -> u8 {
    ((f32::from(v) / 255.0).sqrt() * 255.0).round() as u8
}

pub fn radiation_pressure(src: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.extend(src.iter().map(|&v| apply(v)));
}

pub fn radiation_pressure_inplace(samples: &mut [u8]) {
    for v in samples.iter_mut() {
        *v = apply(*v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radiation_pressure_matches_legacy_formula() {
        let mut out = vec![1, 2, 3];
        radiation_pressure(&[0, 64, 128, 191, 255], &mut out);
        assert_eq!(out, [0, 128, 181, 221, 255]);
    }

    #[test]
    fn radiation_pressure_empty() {
        let mut out = vec![1, 2, 3];
        radiation_pressure(&[], &mut out);
        assert_eq!(out, []);
    }

    #[test]
    fn radiation_pressure_inplace_matches_legacy_formula() {
        let mut buf = [0, 64, 128, 191, 255];
        radiation_pressure_inplace(&mut buf);
        assert_eq!(buf, [0, 128, 181, 221, 255]);
    }

    #[test]
    fn radiation_pressure_inplace_empty() {
        let mut buf: [u8; 0] = [];
        radiation_pressure_inplace(&mut buf);
        assert_eq!(buf, []);
    }
}

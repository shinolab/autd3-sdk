pub fn constant(intensity: u8, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(2);
    out.push(intensity);
    out.push(intensity);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_fills_two_constant_samples() {
        let mut buf = Vec::new();
        constant(0xAB, &mut buf);
        assert_eq!(buf.as_slice(), &[0xAB, 0xAB]);
    }

    #[test]
    fn constant_clears_previous_contents() {
        let mut buf = vec![1, 2, 3, 4, 5];
        constant(0x00, &mut buf);
        assert_eq!(buf.as_slice(), &[0x00, 0x00]);
    }
}

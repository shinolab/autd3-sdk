pub fn constant(amplitude: u8, dst: &mut Vec<u8>) {
    dst.clear();
    dst.reserve(2);
    dst.push(amplitude);
    dst.push(amplitude);
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::fills_two_constant_samples(&[0xAB, 0xAB], 0xAB, vec![])]
    #[case::clears_previous_contents(&[0x00, 0x00], 0x00, vec![1, 2, 3, 4, 5])]
    fn constant_fills_two_samples(
        #[case] expected: &[u8],
        #[case] amplitude: u8,
        #[case] mut buf: Vec<u8>,
    ) {
        constant(amplitude, &mut buf);
        assert_eq!(expected, buf.as_slice());
    }
}

pub const FRAME_HEADER_BYTES: usize = 2;
pub const FRAME_BYTES_MAX: usize = 1448;
pub const PAYLOAD_BYTES: usize = FRAME_BYTES_MAX - FRAME_HEADER_BYTES;
pub const REPLY_DATA_BYTES_MAX: usize = 32;

#[must_use]
pub fn trimmed_len(frame: &[u8]) -> usize {
    frame
        .iter()
        .rposition(|&b| b != 0)
        .map_or(0, |last| last + 1)
        .max(FRAME_HEADER_BYTES.min(frame.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payload_fills_one_ethernet_frame() {
        assert_eq!(PAYLOAD_BYTES, 1446);
    }

    #[test]
    fn trailing_zeros_are_trimmed_but_the_header_stays() {
        assert_eq!(trimmed_len(&[0, 0, 0, 0]), 2);
        assert_eq!(trimmed_len(&[5, 0x04, 0, 0]), 2);
        assert_eq!(trimmed_len(&[5, 0x10, 1, 0, 2, 0, 0]), 5);
        assert_eq!(trimmed_len(&[5, 0x10, 0, 0, 0, 0, 7]), 7);
        assert_eq!(trimmed_len(&[0]), 1);
    }
}

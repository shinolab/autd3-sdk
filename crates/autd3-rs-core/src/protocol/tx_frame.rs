use super::{Cmd, FRAME_BYTES_MAX, FRAME_HEADER_BYTES, PAYLOAD_BYTES, Seq};

#[derive(Clone)]
pub struct TxFrame {
    pub seq: Seq,
    pub cmd: Cmd,
    pub payload: [u8; PAYLOAD_BYTES],
    pub payload_len: usize,
}

impl TxFrame {
    #[must_use]
    pub fn new(seq: Seq, cmd: Cmd) -> Self {
        Self {
            seq,
            cmd,
            payload: [0; PAYLOAD_BYTES],
            payload_len: 0,
        }
    }

    #[must_use]
    pub fn with_payload(seq: Seq, cmd: Cmd, payload: &[u8]) -> Self {
        let mut frame = Self::new(seq, cmd);
        frame.payload[..payload.len()].copy_from_slice(payload);
        frame.payload_len = payload.len();
        frame
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }

    #[must_use]
    pub fn frame_len(&self) -> usize {
        FRAME_HEADER_BYTES + self.payload_len
    }

    pub fn write_to(&self, dst: &mut [u8; FRAME_BYTES_MAX]) -> usize {
        dst[0] = self.seq.get();
        dst[1] = self.cmd.as_u8();
        dst[FRAME_HEADER_BYTES..self.frame_len()].copy_from_slice(self.payload());
        self.frame_len()
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        let mut bytes = [0u8; FRAME_BYTES_MAX];
        let len = self.write_to(&mut bytes);
        bytes[..len].to_vec()
    }

    #[must_use]
    pub fn parse(src: &[u8]) -> Option<Self> {
        let (&[seq, raw_cmd], body) = src.split_first_chunk::<FRAME_HEADER_BYTES>()?;
        let cmd = Cmd::try_from(raw_cmd).ok()?;
        (body.len() <= PAYLOAD_BYTES).then(|| Self::with_payload(Seq::new(seq), cmd, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_frame_round_trips() {
        let mut payload = [0u8; PAYLOAD_BYTES];
        let mut counter: u8 = 0;
        for b in &mut payload {
            *b = counter;
            counter = counter.wrapping_add(1);
        }
        let f = TxFrame::with_payload(Seq::new(0xA5), Cmd::WritePatternRaw, &payload);
        let mut bytes = [0u8; FRAME_BYTES_MAX];
        assert_eq!(f.write_to(&mut bytes), FRAME_BYTES_MAX);
        assert_eq!(bytes[0], 0xA5);
        assert_eq!(bytes[1], Cmd::WritePatternRaw.as_u8());

        let parsed = TxFrame::parse(&bytes).unwrap();
        assert_eq!(parsed.seq, Seq::new(0xA5));
        assert_eq!(parsed.cmd, Cmd::WritePatternRaw);
        assert_eq!(parsed.payload(), &payload[..]);
    }

    #[test]
    fn tx_frame_keeps_trailing_zeros() {
        let f = TxFrame::with_payload(Seq::new(1), Cmd::WriteModulationBuffer, &[7, 0, 0]);
        assert_eq!(f.to_vec(), [1, Cmd::WriteModulationBuffer.as_u8(), 7, 0, 0]);
        let parsed = TxFrame::parse(&f.to_vec()).unwrap();
        assert_eq!(parsed.payload(), [7, 0, 0]);
    }

    #[test]
    fn tx_frame_parse_rejects_unknown_cmd_and_short_frames() {
        assert!(TxFrame::parse(&[0x10, 0xFE]).is_none());
        assert!(TxFrame::parse(&[0x10]).is_none());
    }
}

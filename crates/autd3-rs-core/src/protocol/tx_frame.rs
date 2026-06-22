use super::{Cmd, PAYLOAD_BYTES, Seq, TX_FRAME_BYTES};

#[repr(C)]
#[derive(Clone)]
pub struct TxFrame {
    pub seq: Seq,
    pub cmd: Cmd,
    pub payload: [u8; PAYLOAD_BYTES],
}

const _: () = assert!(size_of::<TxFrame>() == TX_FRAME_BYTES);

impl TxFrame {
    #[must_use]
    pub fn new(seq: Seq, cmd: Cmd) -> Self {
        Self {
            seq,
            cmd,
            payload: [0; PAYLOAD_BYTES],
        }
    }

    pub fn write_to(&self, dst: &mut [u8; TX_FRAME_BYTES]) {
        dst[0] = self.seq.get();
        dst[1] = self.cmd.as_u8();
        dst[2..].copy_from_slice(&self.payload);
    }

    pub fn parse(src: &[u8; TX_FRAME_BYTES]) -> Result<Self, u8> {
        let cmd = Cmd::try_from(src[1])?;
        let mut payload = [0u8; PAYLOAD_BYTES];
        payload.copy_from_slice(&src[2..]);
        Ok(Self {
            seq: Seq::new(src[0]),
            cmd,
            payload,
        })
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
        let f = TxFrame {
            seq: Seq::new(0xA5),
            cmd: Cmd::XorHash,
            payload,
        };
        let mut bytes = [0u8; TX_FRAME_BYTES];
        f.write_to(&mut bytes);
        assert_eq!(bytes[0], 0xA5);
        assert_eq!(bytes[1], Cmd::XorHash.as_u8());

        let parsed = TxFrame::parse(&bytes).unwrap();
        assert_eq!(parsed.seq, Seq::new(0xA5));
        assert_eq!(parsed.cmd, Cmd::XorHash);
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn tx_frame_parse_rejects_unknown_cmd() {
        let mut bytes = [0u8; TX_FRAME_BYTES];
        bytes[0] = 0x10;
        bytes[1] = 0xFE;
        assert!(matches!(TxFrame::parse(&bytes), Err(0xFE)));
    }
}

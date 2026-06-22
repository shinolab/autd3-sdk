use crate::error::{Error, PayloadError};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

pub const XOR_HASH_HEADER_BYTES: usize = 4;

pub const XOR_HASH_MAX_DATA_LEN: usize = PAYLOAD_BYTES - XOR_HASH_HEADER_BYTES;

#[derive(Clone, Debug)]
pub struct XorHashCmd {
    pub sleep_ms: u16,
    pub data: Vec<u8>,
}

impl XorHashCmd {
    #[must_use]
    pub fn with_checksum(sleep_ms: u16, mut payload: Vec<u8>) -> Self {
        let checksum = payload.iter().fold(0u8, |acc, b| acc ^ *b);
        payload.push(checksum);
        Self {
            sleep_ms,
            data: payload,
        }
    }

    #[must_use]
    pub fn computed_xor(&self) -> u8 {
        self.data.iter().fold(0u8, |acc, b| acc ^ *b)
    }
}

impl Operation for XorHashCmd {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        if self.data.len() > XOR_HASH_MAX_DATA_LEN {
            return Err(Error::InvalidPayload(PayloadError::XorHashDataTooLarge {
                len: self.data.len(),
                max: XOR_HASH_MAX_DATA_LEN,
            }));
        }
        let data_len = u16::try_from(self.data.len()).expect("bounded by XOR_HASH_MAX_DATA_LEN");

        out[0..2].copy_from_slice(&self.sleep_ms.to_le_bytes());
        out[2..4].copy_from_slice(&data_len.to_le_bytes());
        out[XOR_HASH_HEADER_BYTES..XOR_HASH_HEADER_BYTES + self.data.len()]
            .copy_from_slice(&self.data);
        Ok(Cmd::XorHash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(op: &XorHashCmd) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out)?;
        Ok((cmd, out))
    }

    #[test]
    fn with_checksum_makes_xor_zero() {
        let cmd = XorHashCmd::with_checksum(0, vec![0x12, 0x34, 0x56]);
        assert_eq!(cmd.computed_xor(), 0);
    }

    #[test]
    fn raw_data_xor_is_observable() {
        let cmd = XorHashCmd {
            sleep_ms: 0,
            data: vec![0xAA],
        };
        assert_eq!(cmd.computed_xor(), 0xAA);
    }

    #[test]
    fn datagram_lays_out_header_correctly() {
        let cmd = XorHashCmd {
            sleep_ms: 0x1234,
            data: vec![0xAA, 0xBB, 0xCC],
        };
        let (cmd_id, payload) = encode(&cmd).unwrap();
        assert_eq!(cmd_id, Cmd::XorHash);
        assert_eq!(&payload[0..2], &0x1234u16.to_le_bytes());
        assert_eq!(&payload[2..4], &3u16.to_le_bytes());
        assert_eq!(&payload[4..7], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn datagram_rejects_oversize_data() {
        let cmd = XorHashCmd {
            sleep_ms: 0,
            data: vec![0; XOR_HASH_MAX_DATA_LEN + 1],
        };
        assert!(matches!(encode(&cmd), Err(Error::InvalidPayload(_))));
    }
}

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const FRAME_BYTES_MAX: usize = 1448;
pub const PAYLOAD_BYTES: usize = FRAME_BYTES_MAX - size_of::<FrameHeader>();
pub const REPLY_DATA_BYTES_MAX: usize = 40;
pub const EXEC_AT_UNIT_NS: u64 = 1_000;

const _: () = assert!(PAYLOAD_BYTES == 1442);

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy)]
#[repr(C)]
pub struct FrameHeader {
    pub reserved: u8,
    pub cmd: u8,
    pub exec_at: U32,
}

impl FrameHeader {
    #[must_use]
    pub fn parse(frame: &[u8]) -> Option<(&Self, &[u8])> {
        let (header, payload) = Self::ref_from_prefix(frame).ok()?;
        (payload.len() <= PAYLOAD_BYTES).then_some((header, payload))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReplyData {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

const _: () = assert!(REPLY_DATA_BYTES_MAX <= u8::MAX as usize);

impl ReplyData {
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; REPLY_DATA_BYTES_MAX],
    };

    #[must_use]
    pub fn from_slice(data: &[u8]) -> Self {
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let mut bytes = [0; REPLY_DATA_BYTES_MAX];
        bytes[..len].copy_from_slice(&data[..len]);
        Self {
            len: len as u8,
            bytes,
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_data_keeps_the_bytes_it_was_given() {
        assert_eq!(ReplyData::from_slice(&[7, 8]).as_slice(), [7, 8]);
        assert_eq!(ReplyData::from_slice(&[]), ReplyData::EMPTY);
    }

    #[test]
    fn reply_data_is_clamped_to_the_reply_limit() {
        let data = ReplyData::from_slice(&[7; REPLY_DATA_BYTES_MAX + 5]);
        assert_eq!(data.as_slice(), [7; REPLY_DATA_BYTES_MAX]);
    }
}

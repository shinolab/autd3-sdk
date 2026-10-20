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

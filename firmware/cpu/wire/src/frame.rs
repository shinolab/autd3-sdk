pub const FRAME_HEADER_BYTES: usize = 2;
pub const FRAME_BYTES_MAX: usize = 1448;
pub const PAYLOAD_BYTES: usize = FRAME_BYTES_MAX - FRAME_HEADER_BYTES;
pub const REPLY_DATA_BYTES_MAX: usize = 32;

const _: () = assert!(PAYLOAD_BYTES == 1446);

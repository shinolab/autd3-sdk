mod seq;
mod tx_frame;

pub use autd3_cpu_wire::{
    Cmd, Error as DeviceErrorCode, FRAME_BYTES_MAX, FRAME_HEADER_BYTES, PAYLOAD_BYTES,
    REPLY_DATA_BYTES_MAX, describe_device_error, trimmed_len,
};
pub use seq::Seq;
pub use tx_frame::TxFrame;

pub const MAX_INFLIGHT: usize = 127;

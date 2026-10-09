mod seq;

pub use autd3_cpu_wire::{
    Cmd, Error as DeviceErrorCode, FRAME_BYTES_MAX, FrameHeader, PAYLOAD_BYTES,
    REPLY_DATA_BYTES_MAX, describe_device_error,
};
pub use seq::Seq;

pub const MAX_INFLIGHT: usize = autd3_cpu_wire::udp::DEVICE_QUEUE_FRAMES;

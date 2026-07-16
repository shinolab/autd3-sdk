mod rx_frame;
mod seq;
mod tx_frame;

pub use autd3_cpu_wire::{
    Cmd, DEVICE_TO_HOST_BYTES as RX_FRAME_BYTES, Error as DeviceErrorCode,
    HOST_TO_DEVICE_BYTES as TX_FRAME_BYTES, PAYLOAD_BYTES, describe_device_error,
};
pub use rx_frame::RxFrame;
pub use seq::Seq;
pub use tx_frame::TxFrame;

pub const MAX_IN_FLIGHT: usize = 127;

pub const MODE_FIFO: u8 = autd3_cpu_wire::Mode::Fifo as u8;
pub const MODE_LOW_LATENCY: u8 = autd3_cpu_wire::Mode::LowLatency as u8;

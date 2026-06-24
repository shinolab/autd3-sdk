mod cmd;
mod error_detail;
mod rx_frame;
mod seq;
mod tx_frame;

pub use cmd::Cmd;
pub use error_detail::{DeviceErrorCode, describe_device_error};
pub use rx_frame::RxFrame;
pub use seq::Seq;
pub use tx_frame::TxFrame;

pub const TX_FRAME_BYTES: usize = 626;
pub const PAYLOAD_BYTES: usize = 624;
pub const RX_FRAME_BYTES: usize = 2;

pub const MAX_IN_FLIGHT: usize = 127;

pub const MODE_FIFO: u8 = 0x00;
pub const MODE_LOW_LATENCY: u8 = 0x01;

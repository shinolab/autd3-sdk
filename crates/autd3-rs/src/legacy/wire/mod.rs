mod fpga_state;
mod frame;
mod gpio;
pub mod params;
mod segment;
mod tag;
mod version;

pub use fpga_state::FpgaState;
pub use frame::{Ack, MsgId, PAYLOAD_BYTES, RX_FRAME_BYTES, RxFrame, TX_FRAME_BYTES, TxFrame};
pub use gpio::GpioOut;
pub use segment::{Segment, TransitionMode};
pub use tag::{GainStmMode, InfoType, Tag};
pub use version::{FirmwareVersion, Version};

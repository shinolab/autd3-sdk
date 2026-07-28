pub mod emulator;
pub mod error;
pub mod op;

mod client;
mod command;
mod datagram;
mod golden;
mod wire;

pub use client::{LegacyClient, LegacyClientConfig, LegacyResponse, MAX_DEVICES};
pub use command::{LegacyChangePatternBank, LegacyCommand};
pub use datagram::{LegacyDatagramBuilder, LegacyFrame, LegacyFrameIter, LegacyFrames};
pub use error::{LegacyError, PayloadError, TimeoutPhase};
pub use wire::{
    FirmwareVersion, FpgaState, GainStmMode, RX_FRAME_BYTES, Segment, TX_FRAME_BYTES,
    TransitionMode, Version,
};

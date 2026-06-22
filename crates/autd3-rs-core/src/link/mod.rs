mod cycle_outcome;
mod device_state;
mod interface;
mod into_link;
mod state_check;
mod stats;
mod status;

pub use cycle_outcome::CycleOutcome;
pub use device_state::DeviceState;
pub use interface::Interface;
pub use into_link::IntoLink;
pub use state_check::{ConstStateChecker, StateCheck};
pub use stats::LinkStats;
pub use status::LinkStatus;

use crate::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES};

pub trait Link: Send + 'static {
    type Error: core::fmt::Display + Send + Sync + 'static;
    type Checker: StateCheck;

    fn num_devices(&self) -> usize;

    fn stats(&self) -> LinkStats {
        LinkStats::default()
    }

    fn state_checker(&self) -> Self::Checker;

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error>;
}

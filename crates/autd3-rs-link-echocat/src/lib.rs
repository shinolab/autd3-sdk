#[doc(hidden)]
pub mod bus;
#[doc(hidden)]
pub mod master;
#[doc(hidden)]
pub mod reg;
#[doc(hidden)]
pub mod sim;
#[doc(hidden)]
pub mod wire;

mod error;
mod link;
mod option;
mod timer;

pub use error::EchocatError;
pub use link::{EchocatLink, StateChecker};
pub use master::{FramePhase, SleepStrategy, WireTiming};
pub use option::{EchocatLinkOption, MAX_DC_START_DELAY, MAX_SYNC_TOLERANCE, MAX_SYNC0_PERIOD};

#[doc(hidden)]
pub use bus::{RawBus, RawSocket};
#[doc(hidden)]
pub use master::{CycleReport, Master, MasterConfig};

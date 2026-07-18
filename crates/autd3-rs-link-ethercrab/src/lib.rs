mod diagnostics;
mod error;
mod join;
mod link;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod option;
mod rt;
mod state_check;
mod status;
mod sync;
mod timeout;
mod timer;
mod transport;
mod utils;
#[cfg(target_os = "windows")]
mod windows;

pub use crate::error::EtherCrabLinkError;
pub use crate::link::EtherCrabLink;
pub use crate::option::{EtherCrabLinkOption, EtherCrabLinkOptionFull};
pub use crate::rt::{CoreId, RtSchedulePolicy, ThreadPriority, ThreadPriorityValue};
pub use crate::state_check::StateChecker;

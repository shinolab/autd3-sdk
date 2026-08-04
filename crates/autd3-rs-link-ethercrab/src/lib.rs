//! `Link` implementation for [`autd3-rs`](https://docs.rs/autd3-rs) backed by
//! [EtherCrab](https://docs.rs/ethercrab), a pure-Rust EtherCAT main device.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod diagnostics;
mod error;
mod join;
mod link;
mod option;
mod osal;
mod pacing;
mod state_check;
mod status;
mod sync;
mod timeout;
mod transport;
mod utils;

pub use crate::error::EtherCrabLinkError;
pub use crate::link::EtherCrabLink;
pub use crate::option::{EtherCrabLinkOption, EtherCrabLinkOptionFull};
pub use crate::osal::thread::{CoreId, RtSchedulePolicy, ThreadPriority, ThreadPriorityValue};
pub use crate::state_check::StateChecker;

// GPL-3.0-only: statically links SOEM. See README.md.

mod adapters;
mod bindings;
mod context;
mod diagnostics;
mod error;
mod link;
mod option;
mod state;
mod state_check;
mod sync;
mod timer;

pub use crate::error::SoemLinkError;
pub use crate::link::SoemLink;
pub use crate::option::{SoemLinkOption, SoemLinkOptionFull};
pub use crate::state_check::StateChecker;

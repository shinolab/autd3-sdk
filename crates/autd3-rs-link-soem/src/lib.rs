//! `Link` implementation for [`autd3-rs`](https://docs.rs/autd3-rs) backed by
//! [SOEM](https://github.com/OpenEtherCATsociety/SOEM).
//!
//! **This crate is GPL-3.0-only**, unlike the MIT-licensed rest of the sdk, because it statically
//! links SOEM. For an MIT transport use
//! [`autd3-rs-link-ethercrab`](https://docs.rs/autd3-rs-link-ethercrab).
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

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

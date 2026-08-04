//! `Link` implementation for [`autd3-rs`](https://docs.rs/autd3-rs) backed by TwinCAT3 via the
//! ADS protocol.
//!
//! Windows only; TwinCAT3 must be installed and configured for the AUTD3 devices.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod error;
mod link;
mod state_check;

pub use ads::{AmsNetId, Timeouts};
pub use error::TwinCATLinkError;
pub use link::{TwinCATLink, TwinCATLinkOption, TwinCATServer};
pub use state_check::TwinCATStateChecker;

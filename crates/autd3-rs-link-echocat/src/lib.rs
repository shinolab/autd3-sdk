//! `Link` implementation for [`autd3-rs`](https://docs.rs/autd3-rs) backed by *echocat*, an
//! EtherCAT main device written for
//! [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display).
//!
//! MIT licensed and dependency-free on the EtherCAT side, so this is the default transport.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

pub mod bus;
pub mod master;
pub mod reg;
pub mod sim;
pub mod wire;

mod error;
mod link;
mod option;
mod timer;

pub use bus::{RawBus, RawSocket};
pub use error::EchocatError;
pub use link::{EchocatLink, StateChecker};
pub use master::{CycleReport, Master, MasterConfig, SleepStrategy};
pub use option::EchocatLinkOption;

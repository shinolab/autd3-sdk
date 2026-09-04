#![no_std]
#![allow(clippy::cast_possible_truncation)]

#[cfg(test)]
extern crate std;

mod app;
mod cmd;
mod fifo;
pub mod fpga;
#[cfg(all(test, loom))]
mod loom_tests;
pub mod params;
pub mod port;
pub mod proto;
mod sync;
#[cfg(all(test, not(loom)))]
mod tests;
pub mod version;

pub use app::Cpu;
pub use port::Port;
pub use version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

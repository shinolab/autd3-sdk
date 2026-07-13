#![no_std]
#![allow(clippy::cast_possible_truncation)]

#[cfg(test)]
extern crate std;

mod app;
mod cmd;
pub mod fpga;
pub mod params;
pub mod port;
pub mod proto;
#[cfg(test)]
mod tests;
pub mod version;

pub use app::{Cpu, FIFO_DEPTH};
pub use port::Port;
pub use version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

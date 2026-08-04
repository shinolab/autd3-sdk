//! Portable logic of the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display) CPU
//! board firmware: protocol handling, command dispatch, and FPGA access.
//!
//! `no_std`, no heap, and free of `unsafe` — hardware access goes through a `Port` trait, so the
//! same sources run on the real board, in host tests, and inside
//! [`autd3-rs-firmware-emulator`](https://docs.rs/autd3-rs-firmware-emulator).
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

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

pub use app::Cpu;
pub use port::Port;
pub use version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

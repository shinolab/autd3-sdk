//! Software emulator of the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display)
//! device firmware.
//!
//! It runs the real CPU firmware sources against a Rust model of the FPGA, so tests can exercise
//! the wire protocol and the emission pipeline without hardware.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod audit;
mod device;
mod emu_fpga;
mod emu_port;
mod fw;

pub use audit::Audit;
pub use device::Device;
pub use emu_fpga::{FpgaEmulator, SilencerEmulator};

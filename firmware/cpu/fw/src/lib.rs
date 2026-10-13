#![no_std]
#![allow(clippy::cast_possible_truncation)]

#[cfg(test)]
extern crate std;

mod app;
mod cmd;
mod ctx;
mod fifo;
pub mod fpga;
pub mod fpga_params;
pub mod net;
pub mod nic;
pub mod node;
pub mod port;
pub mod proto;
pub mod ptp;
#[cfg(test)]
mod sim_nic;
mod sync;
#[cfg(all(test, not(loom)))]
mod test_utils;
pub mod version;

pub use app::Cpu;
pub use autd3_cpu_wire::{fpga_update, udp, update};
pub use port::Port;
pub use version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

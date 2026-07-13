#![allow(clippy::cast_possible_truncation)]

use crate::fw;

#[must_use]
pub const fn cpu_fw_version() -> (u8, u8, u8) {
    (
        fw::FW_VERSION_MAJOR,
        fw::FW_VERSION_MINOR,
        fw::FW_VERSION_PATCH,
    )
}

#[must_use]
pub const fn fpga_fw_version() -> (u16, u16, u16) {
    (
        fw::VERSION_NUM_MAJOR as u16,
        fw::VERSION_NUM_MINOR as u16,
        fw::VERSION_NUM_PATCH as u16,
    )
}

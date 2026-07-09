#![allow(clippy::cast_possible_truncation)]

use crate::ffi;

#[must_use]
pub const fn cpu_fw_version() -> (u8, u8, u8) {
    (
        ffi::FW_VERSION_MAJOR as u8,
        ffi::FW_VERSION_MINOR as u8,
        ffi::FW_VERSION_PATCH as u8,
    )
}

#[must_use]
pub const fn fpga_fw_version() -> (u16, u16, u16) {
    (
        ffi::VERSION_NUM_MAJOR as u16,
        ffi::VERSION_NUM_MINOR as u16,
        ffi::VERSION_NUM_PATCH as u16,
    )
}

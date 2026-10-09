use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::REPLY_DATA_BYTES_MAX;

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct FirmwareInfo {
    pub cpu_version: [u8; 3],
    pub fpga_version: [u8; 3],
    pub fpga_functions: u8,
    pub fpga_boot_image: u8,
}

const _: () = assert!(core::mem::size_of::<FirmwareInfo>() == 8);
const _: () = assert!(core::mem::size_of::<FirmwareInfo>() <= REPLY_DATA_BYTES_MAX);

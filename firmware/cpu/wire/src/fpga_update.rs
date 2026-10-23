use crate::fpga_params::{
    FLASH_END, FLASH_IMAGE_BASE, FLASH_USR_ACCESS_GOLDEN, FLASH_USR_ACCESS_UPDATE,
    FLASH_WRITABLE_BASE,
};

pub const FPGA_FLASH_BYTES: u32 = FLASH_END;
pub const FPGA_SECTOR_BYTES: u32 = 0x1_0000;
pub const FPGA_GOLDEN_REGION_END: u32 = FLASH_WRITABLE_BASE;
pub const FPGA_IMAGE_BASE: u32 = FLASH_IMAGE_BASE;
pub const FPGA_IMAGE_CAPACITY: u32 = FLASH_END - FLASH_IMAGE_BASE;
pub const FPGA_BARRIER_BASE: u32 = FPGA_GOLDEN_REGION_END - 0x100;
pub const FPGA_USR_ACCESS_GOLDEN: u32 = FLASH_USR_ACCESS_GOLDEN;
pub const FPGA_USR_ACCESS_UPDATE: u32 = FLASH_USR_ACCESS_UPDATE;
pub const FPGA_REBOOT_DELAY_MS: u16 = 100;
pub const FPGA_RECONFIG_SETTLE_MS: u16 = 3000;
pub const FPGA_RECONFIG_WORST_MS: u32 =
    FPGA_REBOOT_DELAY_MS as u32 + FPGA_RECONFIG_SETTLE_MS as u32;

const _: () = assert!(FPGA_GOLDEN_REGION_END.is_multiple_of(FPGA_SECTOR_BYTES));
const _: () = assert!(FPGA_IMAGE_BASE >= FPGA_GOLDEN_REGION_END);
const _: () = assert!(FPGA_FLASH_BYTES.is_multiple_of(FPGA_SECTOR_BYTES));

#[must_use]
pub const fn is_plausible_fpga_length(length: u32) -> bool {
    length > 0 && length <= FPGA_IMAGE_CAPACITY
}

crate::wire_enum_u8! {
    pub enum FpgaBootImage {
        Unknown = 0x00,
        Golden = 0x01,
        Update = 0x02,
        ReconfigFailed = 0x03,
    }
}

impl FpgaBootImage {
    #[must_use]
    pub const fn from_usr_access(value: u32) -> Self {
        match value {
            FPGA_USR_ACCESS_GOLDEN => Self::Golden,
            FPGA_USR_ACCESS_UPDATE => Self::Update,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_keeps_golden_below_the_slot() {
        assert_eq!(FPGA_GOLDEN_REGION_END, 0x80_0000);
        assert_eq!(FPGA_IMAGE_BASE, 0x80_0100);
        assert_eq!(FPGA_IMAGE_CAPACITY, 0x7F_FF00);
        assert!(!is_plausible_fpga_length(0));
        assert!(is_plausible_fpga_length(FPGA_IMAGE_CAPACITY));
        assert!(!is_plausible_fpga_length(FPGA_IMAGE_CAPACITY + 1));
        assert_eq!(FPGA_BARRIER_BASE, 0x7F_FF00);
    }

    #[test]
    fn usr_access_names_the_image() {
        assert_eq!(FPGA_USR_ACCESS_GOLDEN.to_be_bytes(), *b"GOLD");
        assert_eq!(FPGA_USR_ACCESS_UPDATE.to_be_bytes(), *b"UPDT");
        assert_eq!(
            FpgaBootImage::from_usr_access(FPGA_USR_ACCESS_GOLDEN),
            FpgaBootImage::Golden
        );
        assert_eq!(
            FpgaBootImage::from_usr_access(FPGA_USR_ACCESS_UPDATE),
            FpgaBootImage::Update
        );
        assert_eq!(
            FpgaBootImage::from_usr_access(0xFFFF_FFFF),
            FpgaBootImage::Unknown
        );
    }
}

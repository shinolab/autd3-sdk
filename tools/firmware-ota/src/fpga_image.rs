use autd3_cpu_wire::fpga_update::{
    FPGA_IMAGE_CAPACITY, FpgaBootImage, FpgaImageError, validate_update_image,
};
use autd3_cpu_wire::update::crc32;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FpgaImageLoadError {
    #[error("FPGA update image must not be empty")]
    Empty,
    #[error("FPGA update image is {len} bytes; the update slot holds at most {max} bytes")]
    TooLarge { len: usize, max: usize },
    #[error(
        "no bitstream sync word (0xAA995566) found; pass the `autd3-fpga-update.img` produced by `cargo xtask fpga build`"
    )]
    NoSyncWord,
    #[error(
        "the bitstream is not an update-slot image (USR_ACCESS reads as {boot_image:?}); pass `autd3-fpga-update.img`, not the golden image"
    )]
    NotAnUpdateImage { boot_image: FpgaBootImage },
    #[error(
        "the bitstream reboots into another image (WBSTAR/IPROG found); a golden image must never be written to the update slot"
    )]
    Reboots,
}

impl From<FpgaImageError> for FpgaImageLoadError {
    fn from(e: FpgaImageError) -> Self {
        match e {
            FpgaImageError::Empty => Self::Empty,
            FpgaImageError::TooLarge { length } => Self::TooLarge {
                len: length,
                max: FPGA_IMAGE_CAPACITY as usize,
            },
            FpgaImageError::NoSyncWord => Self::NoSyncWord,
            FpgaImageError::NotAnUpdateImage { boot_image } => {
                Self::NotAnUpdateImage { boot_image }
            }
            FpgaImageError::Reboots => Self::Reboots,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FpgaFirmwareImage {
    bytes: Vec<u8>,
    crc32: u32,
}

impl FpgaFirmwareImage {
    pub fn from_update_bin(bytes: Vec<u8>) -> Result<Self, FpgaImageLoadError> {
        validate_update_image(&bytes)?;
        let crc32 = crc32(&bytes);
        Ok(Self { bytes, crc32 })
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    #[must_use]
    pub fn crc32(&self) -> u32 {
        self.crc32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_cpu_wire::fpga_update::{FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE, SYNC_WORD};

    fn bitstream(usr_access: u32, reboot: bool, payload: u32) -> Vec<u8> {
        let mut words = vec![
            0xFFFF_FFFF,
            0x0000_00BB,
            0x1122_0044,
            0xFFFF_FFFF,
            SYNC_WORD,
            0x2000_0000,
        ];
        if reboot {
            words.extend([0x3002_0001, 0x0080_0100, 0x3000_8001, 0x0000_000F]);
        }
        words.extend([0x3001_A001, usr_access, 0x3000_4000, 0x5000_0000 | payload]);
        words.extend((0..payload).map(|i| i.wrapping_mul(0x9E37_79B9)));
        words.extend([0x3000_8001, 0x0000_000D, 0x2000_0000]);
        words.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    #[test]
    fn an_update_bitstream_loads() {
        let bytes = bitstream(FPGA_USR_ACCESS_UPDATE, false, 100);
        let image = FpgaFirmwareImage::from_update_bin(bytes.clone()).unwrap();
        assert_eq!(image.len(), bytes.len());
        assert_eq!(image.crc32(), crc32(&bytes));
    }

    #[test]
    fn a_golden_bitstream_is_refused() {
        assert_eq!(
            FpgaFirmwareImage::from_update_bin(bitstream(FPGA_USR_ACCESS_GOLDEN, true, 10)),
            Err(FpgaImageLoadError::Reboots)
        );
        assert_eq!(
            FpgaFirmwareImage::from_update_bin(bitstream(FPGA_USR_ACCESS_GOLDEN, false, 10)),
            Err(FpgaImageLoadError::NotAnUpdateImage {
                boot_image: FpgaBootImage::Golden
            })
        );
    }

    #[test]
    fn garbage_is_refused() {
        assert_eq!(
            FpgaFirmwareImage::from_update_bin(Vec::new()),
            Err(FpgaImageLoadError::Empty)
        );
        assert_eq!(
            FpgaFirmwareImage::from_update_bin(vec![0u8; 64]),
            Err(FpgaImageLoadError::NoSyncWord)
        );
        assert!(matches!(
            FpgaFirmwareImage::from_update_bin(vec![0u8; FPGA_IMAGE_CAPACITY as usize + 1]),
            Err(FpgaImageLoadError::TooLarge { .. })
        ));
    }
}

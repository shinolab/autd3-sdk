use autd3_cpu_wire::update::crc32;
use autd3_rs_core::fpga_bitstream::{FpgaImageError, validate_update_image};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FpgaFirmwareImage {
    bytes: Vec<u8>,
    crc32: u32,
}

impl FpgaFirmwareImage {
    pub(crate) fn from_update_bin(bytes: Vec<u8>) -> Result<Self, FpgaImageError> {
        validate_update_image(&bytes)?;
        let crc32 = crc32(&bytes);
        Ok(Self { bytes, crc32 })
    }

    #[must_use]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub(crate) fn crc32(&self) -> u32 {
        self.crc32
    }
}

use autd3_cpu_wire::update::{
    IMAGE_APP_CAPACITY, IMAGE_VECTOR_BYTES, ImageHeader, Slot, crc32, is_plausible_length,
};
use zerocopy::FromBytes;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("CPU firmware image must not be empty")]
    Empty,
    #[error(
        "CPU firmware image is {len} bytes; it must be longer than {min_exclusive} and at most {max} bytes"
    )]
    LengthOutOfRange {
        len: usize,
        min_exclusive: usize,
        max: usize,
    },
    #[error(
        "flash image is {len} bytes; the slot-A header at 0x{header_at:X} is missing or unstamped"
    )]
    HeaderMissing { len: usize, header_at: usize },
    #[error(
        "flash image slot-A header disagrees with its body (declared {declared} bytes, crc32 0x{crc32:08X})"
    )]
    HeaderMismatch { declared: usize, crc32: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuFirmwareImage {
    body: Vec<u8>,
    crc32: u32,
}

impl CpuFirmwareImage {
    pub fn from_slot_image(body: Vec<u8>) -> Result<Self, ImageError> {
        if body.is_empty() {
            return Err(ImageError::Empty);
        }
        if !u32::try_from(body.len()).is_ok_and(is_plausible_length) {
            return Err(ImageError::LengthOutOfRange {
                len: body.len(),
                min_exclusive: IMAGE_VECTOR_BYTES as usize,
                max: (IMAGE_VECTOR_BYTES + IMAGE_APP_CAPACITY) as usize,
            });
        }
        let crc32 = crc32(&body);
        Ok(Self { body, crc32 })
    }

    pub fn from_flash_image(flash: &[u8]) -> Result<Self, ImageError> {
        let header_at = Slot::A.base() as usize;
        let body_at = Slot::A.image_base() as usize;
        let header = flash
            .get(header_at..header_at + core::mem::size_of::<ImageHeader>())
            .and_then(|raw| ImageHeader::read_from_bytes(raw).ok())
            .filter(ImageHeader::is_plausible)
            .ok_or(ImageError::HeaderMissing {
                len: flash.len(),
                header_at,
            })?;
        let declared = header.length.get() as usize;
        let body = flash
            .get(body_at..body_at + declared)
            .filter(|body| crc32(body) == header.crc32.get())
            .ok_or(ImageError::HeaderMismatch {
                declared,
                crc32: header.crc32.get(),
            })?;
        Self::from_slot_image(body.to_vec())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.body
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.body.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }

    #[must_use]
    pub fn crc32(&self) -> u32 {
        self.crc32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerocopy::IntoBytes;

    fn flash_with(body: &[u8]) -> Vec<u8> {
        let mut flash = vec![0xFFu8; Slot::A.image_base() as usize + body.len()];
        let header = ImageHeader::new(0, u32::try_from(body.len()).unwrap(), crc32(body));
        let at = Slot::A.base() as usize;
        flash[at..at + core::mem::size_of::<ImageHeader>()].copy_from_slice(header.as_bytes());
        flash[Slot::A.image_base() as usize..].copy_from_slice(body);
        flash
    }

    #[test]
    fn slot_image_carries_its_crc() {
        let body = b"123456789".repeat(8);
        let image = CpuFirmwareImage::from_slot_image(body.clone()).unwrap();
        assert_eq!(image.len(), body.len());
        assert_eq!(image.crc32(), crc32(&body));
    }

    #[test]
    fn slot_image_rejects_lengths_the_loader_cannot_copy() {
        assert_eq!(
            CpuFirmwareImage::from_slot_image(Vec::new()),
            Err(ImageError::Empty)
        );
        let max = (IMAGE_VECTOR_BYTES + IMAGE_APP_CAPACITY) as usize;
        for len in [IMAGE_VECTOR_BYTES as usize, max + 1] {
            assert!(matches!(
                CpuFirmwareImage::from_slot_image(vec![0; len]),
                Err(ImageError::LengthOutOfRange { .. })
            ));
        }
        assert!(
            CpuFirmwareImage::from_slot_image(vec![0; IMAGE_VECTOR_BYTES as usize + 1]).is_ok()
        );
        assert!(CpuFirmwareImage::from_slot_image(vec![0; max]).is_ok());
    }

    #[test]
    fn flash_image_extracts_the_stamped_slot_a_body() {
        let body: Vec<u8> = (0..3000u32).map(|i| (i * 7).to_le_bytes()[0]).collect();
        let image = CpuFirmwareImage::from_flash_image(&flash_with(&body)).unwrap();
        assert_eq!(image.as_bytes(), &body[..]);
    }

    #[test]
    fn flash_image_rejects_unstamped_short_or_corrupted_inputs() {
        assert!(matches!(
            CpuFirmwareImage::from_flash_image(&[0xFF; 16]),
            Err(ImageError::HeaderMissing { .. })
        ));
        let unstamped = vec![0xFF; Slot::A.image_base() as usize + 100];
        assert!(matches!(
            CpuFirmwareImage::from_flash_image(&unstamped),
            Err(ImageError::HeaderMissing { .. })
        ));
        let mut flash = flash_with(&[1u8; 500]);
        let last = flash.len() - 1;
        flash[last] ^= 0xFF;
        assert!(matches!(
            CpuFirmwareImage::from_flash_image(&flash),
            Err(ImageError::HeaderMismatch { .. })
        ));
    }
}

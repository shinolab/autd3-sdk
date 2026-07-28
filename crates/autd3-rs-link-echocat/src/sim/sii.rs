pub const SII_BYTES: usize = 2048;

pub use crate::reg::{
    SII_WORD_PRODUCT_CODE as WORD_PRODUCT_CODE, SII_WORD_REVISION as WORD_REVISION,
    SII_WORD_SERIAL as WORD_SERIAL, SII_WORD_VENDOR_ID as WORD_VENDOR_ID,
};

pub const WORD_RX_MAILBOX_OFFSET: u16 = 0x0018;
pub const WORD_STD_RX_MAILBOX_OFFSET: u16 = 0x001c;
pub const WORD_MAILBOX_PROTOCOL: u16 = 0x0020;
pub const WORD_EEPROM_SIZE: u16 = 0x003e;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub vendor: u32,
    pub product: u32,
    pub revision: u32,
    pub serial: u32,
}

pub struct SiiImage {
    bytes: [u8; SII_BYTES],
}

impl SiiImage {
    #[must_use]
    pub fn autd3(identity: Identity) -> Self {
        let mut image = Self {
            bytes: [0; SII_BYTES],
        };
        image.set_word(0x0000, 0x3e80);
        image.set_word(0x0001, 0xaa00);
        image.set_word(0x0003, 0x0001);
        image.set_u32(WORD_VENDOR_ID, identity.vendor);
        image.set_u32(WORD_PRODUCT_CODE, identity.product);
        image.set_u32(WORD_REVISION, identity.revision);
        image.set_u32(WORD_SERIAL, identity.serial);
        image.set_word(WORD_RX_MAILBOX_OFFSET, 0x1000);
        image.set_word(WORD_RX_MAILBOX_OFFSET + 1, 0x0080);
        image.set_word(WORD_RX_MAILBOX_OFFSET + 2, 0x1400);
        image.set_word(WORD_RX_MAILBOX_OFFSET + 3, 0x0080);
        image.set_word(WORD_STD_RX_MAILBOX_OFFSET, 0x1000);
        image.set_word(WORD_STD_RX_MAILBOX_OFFSET + 1, 0x0080);
        image.set_word(WORD_STD_RX_MAILBOX_OFFSET + 2, 0x1400);
        image.set_word(WORD_STD_RX_MAILBOX_OFFSET + 3, 0x0080);
        image.set_word(WORD_MAILBOX_PROTOCOL, 0x000c);
        image.set_word(WORD_EEPROM_SIZE, 0x000f);
        image.set_word(WORD_EEPROM_SIZE + 1, 0x0001);
        image
    }

    fn set_word(&mut self, word: u16, value: u16) {
        let at = usize::from(word) * 2;
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_u32(&mut self, word: u16, value: u32) {
        let at = usize::from(word) * 2;
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[must_use]
    pub fn read(&self, word: u16, len: usize) -> Vec<u8> {
        let at = usize::from(word) * 2;
        let mut out = vec![0u8; len];
        let available = SII_BYTES.saturating_sub(at).min(len);
        out[..available].copy_from_slice(&self.bytes[at..at + available]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Identity, SiiImage, WORD_PRODUCT_CODE, WORD_VENDOR_ID};

    #[test]
    fn identity_words_are_readable_as_little_endian_u32() {
        let image = SiiImage::autd3(Identity {
            vendor: 0x0000_08a9,
            product: 0x0000_0001,
            revision: 0x0000_0001,
            serial: 0,
        });
        assert_eq!(
            u32::from_le_bytes(image.read(WORD_VENDOR_ID, 4).try_into().expect("4 bytes")),
            0x0000_08a9
        );
        assert_eq!(
            u32::from_le_bytes(
                image
                    .read(WORD_PRODUCT_CODE, 4)
                    .try_into()
                    .expect("4 bytes")
            ),
            0x0000_0001
        );
    }

    #[test]
    fn reads_past_the_end_are_zero_filled() {
        let image = SiiImage::autd3(Identity {
            vendor: 1,
            product: 2,
            revision: 3,
            serial: 4,
        });
        assert_eq!(image.read(0x03ff, 4), vec![0, 0, 0, 0]);
    }
}

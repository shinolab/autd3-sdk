use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::{try_read_exact, try_read_header};
use crate::Error;
use crate::frame::{PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};

const fn fits_in_the_bus(addr: u16, words: usize) -> bool {
    words != 0 && addr as usize + words <= 1 << u16::BITS
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug)]
#[repr(C)]
pub struct WriteFpgaMemoryPayload {
    pub addr: U16,
}

impl WriteFpgaMemoryPayload {
    #[must_use]
    pub fn new(addr: u16, words: usize) -> Option<Self> {
        (words <= (PAYLOAD_BYTES - size_of::<Self>()) / size_of::<u16>()
            && fits_in_the_bus(addr, words))
        .then_some(Self { addr: addr.into() })
    }

    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = try_read_header::<Self>(payload)?;
        if !data.len().is_multiple_of(size_of::<u16>())
            || !fits_in_the_bus(p.addr.get(), data.len() / size_of::<u16>())
        {
            return Err(Error::InvalidPayload);
        }
        Ok((p, data))
    }
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug)]
#[repr(C)]
pub struct ReadFpgaMemoryPayload {
    pub addr: U16,
    pub count: U16,
}

impl ReadFpgaMemoryPayload {
    #[must_use]
    pub fn new(addr: u16, count: u16) -> Option<Self> {
        let p = Self {
            addr: addr.into(),
            count: count.into(),
        };
        p.is_valid().then_some(p)
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        if !p.is_valid() {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }

    fn is_valid(self) -> bool {
        let count = usize::from(self.count.get());
        count <= REPLY_DATA_BYTES_MAX / size_of::<u16>() && fits_in_the_bus(self.addr.get(), count)
    }
}

const _: () = assert!(core::mem::size_of::<WriteFpgaMemoryPayload>() == 2);
const _: () = assert!(core::mem::size_of::<ReadFpgaMemoryPayload>() == 4);

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;

    use super::*;

    fn parse_write(addr: u16, data: &[u8]) -> Result<(u16, usize), Error> {
        let mut payload = [0; 8];
        let len = size_of::<WriteFpgaMemoryPayload>() + data.len();
        payload[..2].copy_from_slice(&addr.to_le_bytes());
        payload[2..len].copy_from_slice(data);
        WriteFpgaMemoryPayload::parse(&payload[..len]).map(|(p, data)| (p.addr.get(), data.len()))
    }

    #[test]
    fn a_write_carries_the_rest_of_the_datagram_as_words() {
        let payload = [0x10, 0x40, 0x34, 0x12, 0x78, 0x56];
        let (p, data) = WriteFpgaMemoryPayload::parse(&payload).unwrap();
        assert_eq!(p.addr.get(), 0x4010);
        assert_eq!(data, [0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn a_write_without_words_or_with_half_a_word_is_rejected() {
        for data in [&[][..], &[0x34], &[0x34, 0x12, 0x78]] {
            assert_eq!(parse_write(0, data), Err(Error::InvalidPayload));
        }
        assert_eq!(
            WriteFpgaMemoryPayload::parse(&[0]).err(),
            Some(Error::InvalidPayload)
        );
    }

    #[test]
    fn a_write_must_not_wrap_the_bus_address() {
        assert_eq!(parse_write(0xFFFE, &[0; 4]), Ok((0xFFFE, 4)));
        assert_eq!(parse_write(0xFFFE, &[0; 6]), Err(Error::InvalidPayload));
        assert!(WriteFpgaMemoryPayload::new(0xFFFE, 2).is_some());
        assert!(WriteFpgaMemoryPayload::new(0xFFFE, 3).is_none());
    }

    #[test]
    fn a_write_built_by_the_host_fits_in_a_frame() {
        assert!(WriteFpgaMemoryPayload::new(0, 0).is_none());
        assert!(WriteFpgaMemoryPayload::new(0, 722).is_some());
        assert!(WriteFpgaMemoryPayload::new(0, 723).is_none());
    }

    #[test]
    fn a_read_takes_one_to_twenty_words_inside_the_bus() {
        for (addr, count, accepted) in [
            (0, 0, false),
            (0, 1, true),
            (0, 20, true),
            (0, 21, false),
            (0xFFEC, 20, true),
            (0xFFED, 20, false),
            (0xFFFF, 1, true),
        ] {
            let built = ReadFpgaMemoryPayload::new(addr, count);
            assert_eq!(built.is_some(), accepted, "{addr:#06x} {count}");
            let raw = ReadFpgaMemoryPayload {
                addr: addr.into(),
                count: count.into(),
            };
            assert_eq!(
                ReadFpgaMemoryPayload::parse(raw.as_bytes()).is_ok(),
                accepted,
                "{addr:#06x} {count}"
            );
        }
    }

    #[test]
    fn a_read_requires_the_exact_length() {
        let p = ReadFpgaMemoryPayload::new(0, 1).unwrap();
        assert!(ReadFpgaMemoryPayload::parse(&p.as_bytes()[..3]).is_err());
        assert!(ReadFpgaMemoryPayload::parse(&[0, 0, 1, 0, 0]).is_err());
    }
}

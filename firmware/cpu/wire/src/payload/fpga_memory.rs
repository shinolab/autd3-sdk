use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::{PayloadBuildError, try_read_exact, try_read_header};
use crate::Error;
use crate::frame::{PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};

const fn fits_in_the_bus(addr: u16, words: usize) -> bool {
    words != 0 && addr as usize + words <= 1 << u16::BITS
}

fn check_access(addr: u16, words: usize, max: usize) -> Result<(), PayloadBuildError> {
    if words == 0 {
        return Err(PayloadBuildError::FpgaMemoryAccessEmpty);
    }
    if words > max {
        return Err(PayloadBuildError::FpgaMemoryAccessTooLong { words, max });
    }
    if !fits_in_the_bus(addr, words) {
        return Err(PayloadBuildError::FpgaMemoryAccessExceedsBus { addr, words });
    }
    Ok(())
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Debug)]
#[repr(C)]
pub struct WriteFpgaMemoryPayload {
    pub addr: U16,
}

impl WriteFpgaMemoryPayload {
    pub fn new(addr: u16, words: usize) -> Result<Self, PayloadBuildError> {
        check_access(
            addr,
            words,
            (PAYLOAD_BYTES - size_of::<Self>()) / size_of::<u16>(),
        )?;
        Ok(Self { addr: addr.into() })
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
    pub fn new(addr: u16, count: u16) -> Result<Self, PayloadBuildError> {
        let p = Self {
            addr: addr.into(),
            count: count.into(),
        };
        p.check()?;
        Ok(p)
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        if p.check().is_err() {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }

    fn check(self) -> Result<(), PayloadBuildError> {
        check_access(
            self.addr.get(),
            usize::from(self.count.get()),
            REPLY_DATA_BYTES_MAX / size_of::<u16>(),
        )
    }
}

assert_layout!(WriteFpgaMemoryPayload { addr: 0 } size 2);
assert_layout!(ReadFpgaMemoryPayload { addr: 0, count: 2 } size 4);

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
        assert!(WriteFpgaMemoryPayload::new(0xFFFE, 2).is_ok());
        assert_eq!(
            WriteFpgaMemoryPayload::new(0xFFFE, 3).err(),
            Some(PayloadBuildError::FpgaMemoryAccessExceedsBus {
                addr: 0xFFFE,
                words: 3
            })
        );
    }

    #[test]
    fn a_write_built_by_the_host_fits_in_a_frame() {
        assert_eq!(
            WriteFpgaMemoryPayload::new(0, 0).err(),
            Some(PayloadBuildError::FpgaMemoryAccessEmpty)
        );
        assert!(WriteFpgaMemoryPayload::new(0, 720).is_ok());
        assert_eq!(
            WriteFpgaMemoryPayload::new(0, 721).err(),
            Some(PayloadBuildError::FpgaMemoryAccessTooLong {
                words: 721,
                max: 720
            })
        );
    }

    #[test]
    fn a_read_takes_one_to_twenty_words_inside_the_bus() {
        for (addr, count, expected) in [
            (0, 0, Err(PayloadBuildError::FpgaMemoryAccessEmpty)),
            (0, 1, Ok(())),
            (0, 20, Ok(())),
            (
                0,
                21,
                Err(PayloadBuildError::FpgaMemoryAccessTooLong { words: 21, max: 20 }),
            ),
            (0xFFEC, 20, Ok(())),
            (
                0xFFED,
                20,
                Err(PayloadBuildError::FpgaMemoryAccessExceedsBus {
                    addr: 0xFFED,
                    words: 20,
                }),
            ),
            (0xFFFF, 1, Ok(())),
        ] {
            let accepted = expected.is_ok();
            let built = ReadFpgaMemoryPayload::new(addr, count).map(|_| ());
            assert_eq!(built, expected, "{addr:#06x} {count}");
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

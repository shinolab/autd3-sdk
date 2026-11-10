use autd3_cpu_wire::payload::{ReadFpgaMemoryPayload, WriteFpgaMemoryPayload};

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};

use super::{Operation, encode_header_only, set_cmd, write_header};

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpgaMemoryWrite<'a> {
    pub addr: u16,
    pub words: &'a [u16],
}

impl FpgaMemoryWrite<'_> {
    pub const MAX_LEN: usize =
        (PAYLOAD_BYTES - size_of::<WriteFpgaMemoryPayload>()) / size_of::<u16>();
}

impl crate::sealed::Sealed for FpgaMemoryWrite<'_> {}

impl Operation for FpgaMemoryWrite<'_> {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let header = WriteFpgaMemoryPayload::new(self.addr, self.words.len())?;
        let (data, _) = write_header(out, &header).as_chunks_mut::<2>();
        for (dst, word) in data.iter_mut().zip(self.words) {
            *dst = word.to_le_bytes();
        }
        set_cmd(
            out,
            Cmd::WriteFpgaMemory,
            size_of::<WriteFpgaMemoryPayload>() + size_of_val(self.words),
        );
        Ok(())
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpgaMemoryRead {
    pub addr: u16,
    pub count: u16,
}

impl FpgaMemoryRead {
    pub const MAX_LEN: usize = REPLY_DATA_BYTES_MAX / size_of::<u16>();
}

impl crate::sealed::Sealed for FpgaMemoryRead {}

impl Operation for FpgaMemoryRead {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let payload = ReadFpgaMemoryPayload::new(self.addr, self.count)?;
        encode_header_only(out, Cmd::ReadFpgaMemory, &payload);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use autd3_cpu_wire::payload::PayloadBuildError;

    use super::*;
    use crate::commands::operation::Distribution;
    use crate::error::PayloadError;
    use crate::test_utils::test_device;

    fn encode(op: &dyn Operation) -> Result<Datagram, Error> {
        let mut out = Datagram::no_payload(Cmd::Nop);
        op.encode(&test_device(0), &mut out)?;
        Ok(out)
    }

    #[test]
    fn a_write_carries_the_address_before_the_little_endian_words() {
        let out = encode(&FpgaMemoryWrite {
            addr: 0x4010,
            words: &[0x1234, 0x5678],
        })
        .unwrap();
        assert_eq!(out.cmd, Cmd::WriteFpgaMemory);
        assert_eq!(out.payload(), [0x10, 0x40, 0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn a_read_carries_the_address_and_the_count() {
        let out = encode(&FpgaMemoryRead {
            addr: 0x0102,
            count: 0x0003,
        })
        .unwrap();
        assert_eq!(out.cmd, Cmd::ReadFpgaMemory);
        assert_eq!(out.payload(), [0x02, 0x01, 3, 0]);
    }

    #[test]
    fn a_write_fills_the_frame_at_its_limit() {
        let words = vec![0xABCD; FpgaMemoryWrite::MAX_LEN];
        let out = encode(&FpgaMemoryWrite {
            addr: 0,
            words: &words,
        })
        .unwrap();
        assert_eq!(out.payload_len, PAYLOAD_BYTES);
    }

    #[test]
    fn a_write_outside_the_frame_or_the_bus_is_rejected() {
        let longest = vec![0; FpgaMemoryWrite::MAX_LEN + 1];
        for (addr, words, expected) in [
            (0, &[][..], PayloadBuildError::FpgaMemoryAccessEmpty),
            (
                0,
                &longest,
                PayloadBuildError::FpgaMemoryAccessTooLong {
                    words: FpgaMemoryWrite::MAX_LEN + 1,
                    max: FpgaMemoryWrite::MAX_LEN,
                },
            ),
            (
                0xFFFE,
                &[0, 0, 0],
                PayloadBuildError::FpgaMemoryAccessExceedsBus {
                    addr: 0xFFFE,
                    words: 3,
                },
            ),
        ] {
            assert!(matches!(
                encode(&FpgaMemoryWrite { addr, words }).unwrap_err(),
                Error::InvalidPayload(PayloadError::Build(e)) if e == expected
            ));
        }
        encode(&FpgaMemoryWrite {
            addr: 0xFFFE,
            words: &[0, 0],
        })
        .unwrap();
    }

    #[test]
    fn a_read_outside_the_reply_or_the_bus_is_rejected() {
        let longest = u16::try_from(FpgaMemoryRead::MAX_LEN).unwrap();
        for (addr, count, expected) in [
            (0, 0, PayloadBuildError::FpgaMemoryAccessEmpty),
            (
                0,
                longest + 1,
                PayloadBuildError::FpgaMemoryAccessTooLong {
                    words: FpgaMemoryRead::MAX_LEN + 1,
                    max: FpgaMemoryRead::MAX_LEN,
                },
            ),
            (
                0xFFFF,
                2,
                PayloadBuildError::FpgaMemoryAccessExceedsBus {
                    addr: 0xFFFF,
                    words: 2,
                },
            ),
        ] {
            assert!(matches!(
                encode(&FpgaMemoryRead { addr, count }).unwrap_err(),
                Error::InvalidPayload(PayloadError::Build(e)) if e == expected
            ));
        }
        encode(&FpgaMemoryRead {
            addr: 0,
            count: longest,
        })
        .unwrap();
    }

    #[test]
    fn both_are_broadcast() {
        assert_eq!(
            FpgaMemoryRead { addr: 0, count: 1 }.distribution(),
            Distribution::Broadcast
        );
        assert_eq!(
            FpgaMemoryWrite {
                addr: 0,
                words: &[0]
            }
            .distribution(),
            Distribution::Broadcast
        );
    }
}

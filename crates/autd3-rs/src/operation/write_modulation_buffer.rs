use crate::error::{Error, PayloadError};
use crate::params::MOD_BUFFER_SAMPLES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::ModulationBank;

use super::{Distribution, Operation, WRITE_HEADER_BYTES, WRITE_MAX_DATA_LEN};

use zerocopy::{FromBytes, IntoBytes, KnownLayout, little_endian};

#[repr(C)]
#[derive(FromBytes, IntoBytes, KnownLayout)]
struct WriteHeader {
    bank: u8,
    _reserved: u8,
    offset: little_endian::U32,
    len: little_endian::U16,
}

#[derive(Clone, Copy, Debug)]
pub struct WriteModulationBuffer<'a> {
    pub bank: ModulationBank,
    pub offset: u32,
    pub data: &'a [u8],
}

impl Operation for WriteModulationBuffer<'_> {
    fn frames(&self) -> usize {
        self.data.len().div_ceil(WRITE_MAX_DATA_LEN).max(1)
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        if self.data.is_empty() {
            return Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty));
        }
        if !self.offset.is_multiple_of(2) {
            return Err(Error::InvalidPayload(
                PayloadError::ModulationOffsetNotEven {
                    offset: self.offset,
                },
            ));
        }
        let end = self.offset as usize + self.data.len();
        if end > MOD_BUFFER_SAMPLES {
            return Err(Error::InvalidPayload(
                PayloadError::ModulationWriteExceedsCapacity {
                    offset: self.offset as usize,
                    end,
                    capacity: MOD_BUFFER_SAMPLES,
                },
            ));
        }

        let start = frame * WRITE_MAX_DATA_LEN;
        let chunk = &self.data[start..(start + WRITE_MAX_DATA_LEN).min(self.data.len())];
        let offset = self.offset + u32::try_from(start).expect("bounded by MOD_BUFFER_SAMPLES");
        let len = u16::try_from(chunk.len()).expect("bounded by WRITE_MAX_DATA_LEN");

        let (header, _) =
            WriteHeader::mut_from_prefix(&mut out[..]).expect("WriteHeader fits the payload");
        header.bank = self.bank.as_u8();
        header.offset = offset.into();
        header.len = len.into();
        out[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + chunk.len()].copy_from_slice(chunk);
        Ok(Cmd::WriteModulationBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(op: &WriteModulationBuffer, frame: usize) -> Result<[u8; PAYLOAD_BYTES], Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(0, frame, &mut out)?;
        Ok(out)
    }

    #[test]
    fn write_modulation_buffer_single_frame() {
        let op = WriteModulationBuffer {
            bank: ModulationBank::B1,
            offset: 0x0102,
            data: &[0xAA, 0xBB, 0xCC],
        };

        assert_eq!(op.frames(), 1);
        let p = encode(&op, 0).unwrap();
        assert_eq!(p[0], 1);
        assert_eq!(p[1], 0);
        assert_eq!(&p[2..6], &0x0102u32.to_le_bytes());
        assert_eq!(&p[6..8], &3u16.to_le_bytes());
        assert_eq!(&p[8..11], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn write_modulation_buffer_splits_with_advancing_even_offset() {
        let data: Vec<u8> = (0..1000u16)
            .map(|i| u8::try_from(i % 256).unwrap())
            .collect();
        let op = WriteModulationBuffer {
            bank: ModulationBank::B0,
            offset: 100,
            data: &data,
        };

        assert_eq!(op.frames(), 2);
        assert_eq!(WRITE_MAX_DATA_LEN % 2, 0, "split must keep offsets even");

        let p0 = encode(&op, 0).unwrap();
        assert_eq!(&p0[2..6], &100u32.to_le_bytes());
        let max = u16::try_from(WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p0[6..8], &max.to_le_bytes());
        assert_eq!(
            &p0[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + WRITE_MAX_DATA_LEN],
            &data[..WRITE_MAX_DATA_LEN]
        );

        let p1 = encode(&op, 1).unwrap();
        assert_eq!(&p1[2..6], &(100 + u32::from(max)).to_le_bytes());
        let rest = u16::try_from(1000 - WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
    }

    #[test]
    fn write_modulation_buffer_accepts_exactly_full_capacity() {
        let data = vec![0x55; MOD_BUFFER_SAMPLES];
        let op = WriteModulationBuffer {
            bank: ModulationBank::B0,
            offset: 0,
            data: &data,
        };
        assert_eq!(op.frames(), MOD_BUFFER_SAMPLES.div_ceil(WRITE_MAX_DATA_LEN));
        assert!(encode(&op, 0).is_ok());
    }

    #[test]
    fn write_modulation_buffer_rejects_invalid_inputs() {
        let op = |offset: u32, data: &[u8]| -> Result<[u8; PAYLOAD_BYTES], Error> {
            encode(
                &WriteModulationBuffer {
                    bank: ModulationBank::B0,
                    offset,
                    data,
                },
                0,
            )
        };
        assert!(matches!(op(0, &[]), Err(Error::InvalidPayload(_))));
        assert!(matches!(op(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            op(u32::try_from(MOD_BUFFER_SAMPLES - 2).unwrap(), &[0; 3]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

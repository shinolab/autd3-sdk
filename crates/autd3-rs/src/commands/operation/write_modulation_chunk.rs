use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::MOD_BUFFER_SAMPLES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::ModulationBank;

use super::{Distribution, Operation};
use autd3_cpu_wire::payload::WriteModPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::{U16, U32};

#[derive(Clone, Copy, Debug)]
pub(crate) struct WriteModulationChunk<'a> {
    pub bank: ModulationBank,
    pub offset: usize,
    pub data: &'a [u8],
}

impl Operation for WriteModulationChunk<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        if self.data.is_empty() {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        if !self.offset.is_multiple_of(2) {
            return Err(PayloadError::ModulationOffsetNotEven {
                offset: self.offset,
            }
            .into());
        }
        let end = self.offset + self.data.len();
        if end > MOD_BUFFER_SAMPLES {
            return Err(PayloadError::ModulationWriteExceedsCapacity {
                offset: self.offset,
                end,
                capacity: MOD_BUFFER_SAMPLES,
            }
            .into());
        }

        let offset = u32::try_from(self.offset).expect("bounded by MOD_BUFFER_SAMPLES");
        let len = u16::try_from(self.data.len()).expect("bounded by WRITE_MAX_DATA_LEN");

        let (h, rest) = WriteModPayload::mut_from_prefix(&mut out[..]).unwrap();
        *h = WriteModPayload {
            bank: self.bank.as_u8(),
            reserved: 0,
            offset: U32::new(offset),
            data_len: U16::new(len),
        };
        rest[..self.data.len()].copy_from_slice(self.data);
        Ok(Cmd::WriteModulationBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;
    use autd3_cpu_wire::layout::WRITE_HEADER_BYTES;

    #[test]
    fn write_modulation_chunk_writes_header_and_body() {
        let op = WriteModulationChunk {
            bank: ModulationBank::B1,
            offset: 0x0102,
            data: &[0xAA, 0xBB, 0xCC],
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(cmd, Cmd::WriteModulationBuffer);
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 0);
        assert_eq!(&out[2..6], &0x0102u32.to_le_bytes());
        assert_eq!(&out[6..8], &3u16.to_le_bytes());
        assert_eq!(
            &out[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + 3],
            &[0xAA, 0xBB, 0xCC]
        );
    }

    #[test]
    fn write_modulation_chunk_rejects_invalid_windows() {
        let encode = |offset: usize, data: &[u8]| -> Result<Cmd, Error> {
            let mut out = [0u8; PAYLOAD_BYTES];
            WriteModulationChunk {
                bank: ModulationBank::B0,
                offset,
                data,
            }
            .encode(&test_device(0), &mut out)
        };
        assert!(matches!(encode(0, &[]), Err(Error::InvalidPayload(_))));
        assert!(matches!(encode(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            encode(MOD_BUFFER_SAMPLES - 2, &[0; 3]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

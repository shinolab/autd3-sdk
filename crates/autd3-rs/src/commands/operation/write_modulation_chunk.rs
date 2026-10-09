use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::ModulationBank;

use super::{Encoded, Operation, write_header};
use autd3_cpu_wire::payload::WriteModPayload;

#[derive(Clone, Copy, Debug)]
pub(crate) struct WriteModulationChunk<'a> {
    pub bank: ModulationBank,
    pub offset: usize,
    pub data: &'a [u8],
}

impl crate::sealed::Sealed for WriteModulationChunk<'_> {}

impl Operation for WriteModulationChunk<'_> {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let header = WriteModPayload::new(self.bank, self.offset, self.data.len())?;
        let rest = write_header(out, &header);
        rest[..self.data.len()].copy_from_slice(self.data);
        Ok(Encoded::header_with_data::<WriteModPayload>(
            Cmd::WriteModulationBuffer,
            self.data.len(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MOD_BUFFER_SAMPLES;
    use crate::test_utils::encode;

    #[test]
    fn write_modulation_chunk_writes_header_and_body() {
        let op = WriteModulationChunk {
            bank: ModulationBank::B1,
            offset: 0x0102,
            data: &[0xAA, 0xBB, 0xCC],
        };

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            Encoded::header_with_data::<WriteModPayload>(Cmd::WriteModulationBuffer, 3)
        );
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 0);
        assert_eq!(&out[2..6], &0x0102u32.to_le_bytes());
        assert_eq!(
            &out[size_of::<WriteModPayload>()..][..3],
            &[0xAA, 0xBB, 0xCC]
        );
    }

    #[test]
    fn write_modulation_chunk_rejects_invalid_windows() {
        let chunk = |offset: usize, data: &[u8]| {
            encode(&WriteModulationChunk {
                bank: ModulationBank::B0,
                offset,
                data,
            })
        };
        assert!(matches!(chunk(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            chunk(MOD_BUFFER_SAMPLES - 2, &[0; 3]),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            chunk(usize::MAX - 1, &[0; 2]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

use autd3_cpu_wire::payload::WriteModPayload;

use crate::datagram::Expansion;
use crate::error::{Error, PayloadError};
use crate::protocol::PAYLOAD_BYTES;
use crate::value::ModulationBank;

use super::Command;
use super::operation::WriteModulationChunk;

#[derive(Clone, Copy, Debug)]
pub struct WriteModulationBuffer<'a> {
    pub bank: ModulationBank,
    pub offset: usize,
    pub data: &'a [u8],
}

impl<'a> Command<'a> for WriteModulationBuffer<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        if self.data.is_empty() {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        let max_data_len = PAYLOAD_BYTES - size_of::<WriteModPayload>();
        for (i, chunk) in self.data.chunks(max_data_len).enumerate() {
            expansion.push(WriteModulationChunk {
                bank: self.bank,
                offset: self.offset.saturating_add(i * max_data_len),
                data: chunk,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MOD_BUFFER_SAMPLES;
    use crate::test_utils::{build, payload};

    #[test]
    fn write_modulation_buffer_splits_with_advancing_even_offset() {
        let max_data_len = PAYLOAD_BYTES - size_of::<WriteModPayload>();
        let total = max_data_len + 562;
        let data: Vec<u8> = (0..total).map(|i| u8::try_from(i % 256).unwrap()).collect();
        let frames = build(
            1,
            WriteModulationBuffer {
                bank: ModulationBank::B1,
                offset: 100,
                data: &data,
            },
        )
        .unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(max_data_len % 2, 0, "split must keep offsets even");

        let p0 = payload(&frames, 0, 0);
        assert_eq!(p0[0], 1, "bank B1");
        assert_eq!(&p0[2..6], &100u32.to_le_bytes());
        assert_eq!(&p0[size_of::<WriteModPayload>()..], &data[..max_data_len]);

        let p1 = payload(&frames, 1, 0);
        let max = u32::try_from(max_data_len).unwrap();
        assert_eq!(p1[0], 1, "bank B1");
        assert_eq!(&p1[2..6], &(100 + max).to_le_bytes());
        assert_eq!(&p1[size_of::<WriteModPayload>()..], &data[max_data_len..]);
    }

    #[test]
    fn write_modulation_buffer_accepts_exactly_full_capacity() {
        let data = vec![0x55; MOD_BUFFER_SAMPLES];
        let frames = build(
            1,
            WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset: 0,
                data: &data,
            },
        )
        .unwrap();
        assert_eq!(
            frames.len(),
            MOD_BUFFER_SAMPLES.div_ceil(PAYLOAD_BYTES - size_of::<WriteModPayload>())
        );
    }

    #[test]
    fn write_modulation_buffer_rejects_empty_data() {
        let result = build(
            1,
            WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset: 0,
                data: &[],
            },
        );
        assert!(matches!(
            result,
            Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
        ));
    }
}

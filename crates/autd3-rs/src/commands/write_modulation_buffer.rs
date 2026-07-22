use crate::datagram::DatagramBuilder;
use crate::error::PayloadError;
use crate::value::ModulationBank;

use super::Command;
use super::operation::{WRITE_MAX_DATA_LEN, WriteModulationChunk};

#[derive(Clone, Copy, Debug)]
pub struct WriteModulationBuffer<'a> {
    pub bank: ModulationBank,
    pub offset: usize,
    pub data: &'a [u8],
}

impl<'a> Command<'a> for WriteModulationBuffer<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        if self.data.is_empty() {
            builder.reject(PayloadError::ModulationDataEmpty);
            return;
        }
        for (i, chunk) in self.data.chunks(WRITE_MAX_DATA_LEN).enumerate() {
            builder.push(WriteModulationChunk {
                bank: self.bank,
                offset: self.offset + i * WRITE_MAX_DATA_LEN,
                data: chunk,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datagram::Frames;
    use crate::error::Error;
    use crate::params::MOD_BUFFER_SAMPLES;
    use crate::protocol::PAYLOAD_BYTES;
    use crate::test_utils::test_geometry_arc;
    use autd3_cpu_wire::layout::WRITE_HEADER_BYTES;

    fn expand(op: WriteModulationBuffer<'_>) -> Result<Frames, Error> {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(op);
        b.build()
    }

    fn payload(frames: &Frames, index: usize) -> [u8; PAYLOAD_BYTES] {
        frames.frame(index).unwrap().datagrams()[0].payload
    }

    #[test]
    fn write_modulation_buffer_single_frame() {
        let frames = expand(WriteModulationBuffer {
            bank: ModulationBank::B1,
            offset: 0x0102,
            data: &[0xAA, 0xBB, 0xCC],
        })
        .unwrap();

        assert_eq!(frames.len(), 1);
        let p = payload(&frames, 0);
        assert_eq!(p[0], 1);
        assert_eq!(&p[2..6], &0x0102u32.to_le_bytes());
        assert_eq!(&p[6..8], &3u16.to_le_bytes());
        assert_eq!(&p[8..11], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn write_modulation_buffer_splits_with_advancing_even_offset() {
        let data: Vec<u8> = (0..1000u16)
            .map(|i| u8::try_from(i % 256).unwrap())
            .collect();
        let frames = expand(WriteModulationBuffer {
            bank: ModulationBank::B0,
            offset: 100,
            data: &data,
        })
        .unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(WRITE_MAX_DATA_LEN % 2, 0, "split must keep offsets even");

        let p0 = payload(&frames, 0);
        assert_eq!(&p0[2..6], &100u32.to_le_bytes());
        let max = u16::try_from(WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p0[6..8], &max.to_le_bytes());
        assert_eq!(
            &p0[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + WRITE_MAX_DATA_LEN],
            &data[..WRITE_MAX_DATA_LEN]
        );

        let p1 = payload(&frames, 1);
        assert_eq!(&p1[2..6], &(100 + u32::from(max)).to_le_bytes());
        let rest = u16::try_from(1000 - WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
        assert_eq!(
            &p1[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + usize::from(rest)],
            &data[WRITE_MAX_DATA_LEN..]
        );
    }

    #[test]
    fn write_modulation_buffer_accepts_exactly_full_capacity() {
        let data = vec![0x55; MOD_BUFFER_SAMPLES];
        let frames = expand(WriteModulationBuffer {
            bank: ModulationBank::B0,
            offset: 0,
            data: &data,
        })
        .unwrap();
        assert_eq!(
            frames.len(),
            MOD_BUFFER_SAMPLES.div_ceil(WRITE_MAX_DATA_LEN)
        );
    }

    #[test]
    fn write_modulation_buffer_rejects_invalid_inputs() {
        let op = |offset: usize, data: &[u8]| -> Result<Frames, Error> {
            expand(WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset,
                data,
            })
        };
        assert!(matches!(op(0, &[]), Err(Error::InvalidPayload(_))));
        assert!(matches!(op(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            op(MOD_BUFFER_SAMPLES - 2, &[0; 3]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

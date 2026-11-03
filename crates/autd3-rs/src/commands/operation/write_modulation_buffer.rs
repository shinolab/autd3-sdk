use crate::datagram::Datagram;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::ModulationBank;

use super::{Operation, set_cmd, write_header};
use autd3_cpu_wire::payload::WriteModPayload;

#[derive(Clone, Debug)]
pub struct WriteModulationBuffer<'a> {
    pub bank: ModulationBank,
    pub offset: usize,
    pub data: &'a [u8],
}

impl WriteModulationBuffer<'_> {
    pub const MAX_LEN: usize = PAYLOAD_BYTES - size_of::<WriteModPayload>();
}

impl crate::sealed::Sealed for WriteModulationBuffer<'_> {}

impl Operation for WriteModulationBuffer<'_> {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        if self.data.is_empty() {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        if self.data.len() > Self::MAX_LEN {
            return Err(PayloadError::ModulationWriteTooLong {
                len: self.data.len(),
                max: Self::MAX_LEN,
            }
            .into());
        }
        let header = WriteModPayload::new(self.bank, self.offset, self.data.len())?;
        let rest = write_header(out, &header);
        rest[..self.data.len()].copy_from_slice(self.data);
        set_cmd(
            out,
            Cmd::WriteModulationBuffer,
            size_of::<WriteModPayload>() + self.data.len(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MOD_BUFFER_SAMPLES;
    use crate::test_utils::encode;

    const _: () = assert!(WriteModulationBuffer::MAX_LEN.is_multiple_of(2));

    #[test]
    fn write_modulation_buffer_writes_header_and_body() {
        let op = WriteModulationBuffer {
            bank: ModulationBank::B1,
            offset: 0x0102,
            data: &[0xAA, 0xBB, 0xCC],
        };

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            (Cmd::WriteModulationBuffer, size_of::<WriteModPayload>() + 3)
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
    fn write_modulation_buffer_accepts_exactly_one_frame() {
        let data = vec![0x55; WriteModulationBuffer::MAX_LEN];
        let (cmd, out) = encode(&WriteModulationBuffer {
            bank: ModulationBank::B0,
            offset: 0,
            data: &data,
        })
        .unwrap();
        assert_eq!(cmd.1, PAYLOAD_BYTES);
        assert_eq!(&out[size_of::<WriteModPayload>()..], &data[..]);
    }

    #[test]
    fn write_modulation_buffer_rejects_more_than_one_frame() {
        let data = vec![0x55; WriteModulationBuffer::MAX_LEN + 1];
        assert!(matches!(
            encode(&WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset: 0,
                data: &data,
            }),
            Err(Error::InvalidPayload(PayloadError::ModulationWriteTooLong { len, max }))
                if len == WriteModulationBuffer::MAX_LEN + 1 && max == WriteModulationBuffer::MAX_LEN
        ));
    }

    #[test]
    fn write_modulation_buffer_rejects_empty_data() {
        assert!(matches!(
            encode(&WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset: 0,
                data: &[],
            }),
            Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
        ));
    }

    #[test]
    fn write_modulation_buffer_rejects_invalid_windows() {
        let write = |offset: usize, data: &[u8]| {
            encode(&WriteModulationBuffer {
                bank: ModulationBank::B0,
                offset,
                data,
            })
        };
        assert!(matches!(write(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            write(MOD_BUFFER_SAMPLES - 2, &[0; 3]),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            write(usize::MAX - 1, &[0; 2]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

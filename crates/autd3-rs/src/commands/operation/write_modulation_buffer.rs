use crate::commands::Command;
use crate::datagram::{Datagram, Expansion};
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

impl Command for WriteModulationBuffer<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        if self.data.is_empty() {
            return Err(PayloadError::ModulationDataEmpty.into());
        }
        for (i, data) in self.data.chunks(WriteModulationChunk::MAX_LEN).enumerate() {
            expansion.push_op(&WriteModulationChunk {
                bank: self.bank,
                offset: self
                    .offset
                    .saturating_add(i * WriteModulationChunk::MAX_LEN),
                data,
            })?;
        }
        Ok(())
    }
}

struct WriteModulationChunk<'a> {
    bank: ModulationBank,
    offset: usize,
    data: &'a [u8],
}

impl WriteModulationChunk<'_> {
    const MAX_LEN: usize = (PAYLOAD_BYTES - size_of::<WriteModPayload>()) / 2 * 2;
}

impl crate::sealed::Sealed for WriteModulationChunk<'_> {}

impl Operation for WriteModulationChunk<'_> {
    fn waits_for_previous(&self) -> bool {
        false
    }

    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let len = self.data.len().next_multiple_of(2);
        let header = WriteModPayload::new(self.bank, self.offset, len)?;
        let rest = write_header(out, &header);
        rest[..self.data.len()].copy_from_slice(self.data);
        rest[self.data.len()..len].fill(0);
        set_cmd(
            out,
            Cmd::WriteModulationBuffer,
            size_of::<WriteModPayload>() + len,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MOD_BUFFER_SAMPLES;
    use crate::test_utils::{build, cmds, payload, waits};

    const MAX_LEN: usize = WriteModulationChunk::MAX_LEN;
    const _: () = assert!(MAX_LEN.is_multiple_of(2));
    const _: () = assert!(PAYLOAD_BYTES - size_of::<WriteModPayload>() - MAX_LEN < 2);

    fn write(offset: usize, data: &[u8]) -> Result<crate::datagram::Frames, Error> {
        build(
            1,
            WriteModulationBuffer {
                bank: ModulationBank::B1,
                offset,
                data,
            },
        )
    }

    #[test]
    fn write_modulation_buffer_writes_header_and_body() {
        let frames = write(0x0102, &[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();

        assert_eq!(cmds(&frames), [Cmd::WriteModulationBuffer]);
        let out = payload(&frames, 0, 0);
        assert_eq!(out.len(), size_of::<WriteModPayload>() + 4);
        assert_eq!(out[0], 1);
        assert_eq!(&out[1..5], &0x0102u32.to_le_bytes());
        assert_eq!(
            &out[size_of::<WriteModPayload>()..],
            &[0xAA, 0xBB, 0xCC, 0xDD]
        );
    }

    #[test]
    fn write_modulation_buffer_splits_from_the_given_offset() {
        let data: Vec<u8> = (0..MAX_LEN + 4)
            .map(|i| u8::try_from(i % 256).unwrap())
            .collect();
        let frames = write(4, &data).unwrap();

        assert_eq!(
            cmds(&frames),
            [Cmd::WriteModulationBuffer, Cmd::WriteModulationBuffer]
        );
        assert_eq!(waits(&frames), [false, false]);
        let p0 = payload(&frames, 0, 0);
        assert_eq!(&p0[1..5], &4u32.to_le_bytes());
        assert_eq!(&p0[size_of::<WriteModPayload>()..], &data[..MAX_LEN]);
        let p1 = payload(&frames, 1, 0);
        assert_eq!(
            &p1[1..5],
            &u32::try_from(4 + MAX_LEN).unwrap().to_le_bytes()
        );
        assert_eq!(&p1[size_of::<WriteModPayload>()..], &data[MAX_LEN..]);
    }

    #[test]
    fn write_modulation_buffer_pads_an_odd_tail_with_zero() {
        let data: Vec<u8> = (1..=u8::try_from(MAX_LEN % 256).unwrap())
            .cycle()
            .take(MAX_LEN + 3)
            .collect();
        let frames = write(0, &data).unwrap();

        assert_eq!(
            payload(&frames, 0, 0).len(),
            size_of::<WriteModPayload>() + MAX_LEN
        );
        let tail = &payload(&frames, 1, 0)[size_of::<WriteModPayload>()..];
        assert_eq!(&tail[..3], &data[MAX_LEN..]);
        assert_eq!(tail[3], 0);
        assert_eq!(tail.len(), 4);

        let frames = write(MOD_BUFFER_SAMPLES - 4, &[0x11, 0x22, 0x33]).unwrap();
        assert_eq!(
            &payload(&frames, 0, 0)[size_of::<WriteModPayload>()..],
            &[0x11, 0x22, 0x33, 0x00]
        );
    }

    #[test]
    fn write_modulation_buffer_rejects_empty_data() {
        assert!(matches!(
            write(0, &[]),
            Err(Error::InvalidPayload(PayloadError::ModulationDataEmpty))
        ));
    }

    #[test]
    fn write_modulation_buffer_rejects_invalid_windows() {
        assert!(matches!(write(1, &[0; 2]), Err(Error::InvalidPayload(_))));
        assert!(matches!(
            write(MOD_BUFFER_SAMPLES - 2, &[0; 4]),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            write(usize::MAX - 1, &[0; 2]),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            write(usize::MAX - 1, &vec![0; MAX_LEN + 2]),
            Err(Error::InvalidPayload(_))
        ));
    }
}

use crate::datagram::Datagram;
use crate::error::Error;
use crate::params::MOD_BUFFER_SAMPLES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::Bank;

use super::{Instruction, WRITE_HEADER_BYTES, WRITE_MAX_DATA_LEN};

use zerocopy::{FromBytes, IntoBytes, KnownLayout, little_endian};

#[repr(C)]
#[derive(FromBytes, IntoBytes, KnownLayout)]
struct WriteHeader {
    bank: u8,
    _reserved: u8,
    offset: little_endian::U32,
    len: little_endian::U16,
}

#[derive(Clone, Debug)]
pub struct WriteModulationBuffer {
    pub bank: Bank,
    pub offset: u32,
    pub data: Vec<u8>,
}

impl Instruction for WriteModulationBuffer {
    fn datagrams(&self) -> Result<Vec<Datagram>, Error> {
        if self.data.is_empty() {
            return Err(Error::InvalidPayload(
                "modulation data must not be empty".into(),
            ));
        }
        if !self.offset.is_multiple_of(2) {
            return Err(Error::InvalidPayload(format!(
                "modulation offset {} must be even (word-write-only RAM)",
                self.offset
            )));
        }
        let end = self.offset as usize + self.data.len();
        if end > MOD_BUFFER_SAMPLES {
            return Err(Error::InvalidPayload(format!(
                "modulation write [{}, {end}) exceeds buffer capacity {MOD_BUFFER_SAMPLES}",
                self.offset
            )));
        }

        let mut offset = self.offset;
        let mut datagrams = Vec::with_capacity(self.data.len().div_ceil(WRITE_MAX_DATA_LEN));
        for chunk in self.data.chunks(WRITE_MAX_DATA_LEN) {
            let mut payload = [0u8; PAYLOAD_BYTES];
            let len = u16::try_from(chunk.len()).expect("bounded by WRITE_MAX_DATA_LEN");
            let (header, _) =
                WriteHeader::mut_from_prefix(&mut payload).expect("WriteHeader fits the payload");
            header.bank = self.bank.as_u8();
            header.offset = offset.into();
            header.len = len.into();
            payload[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + chunk.len()].copy_from_slice(chunk);
            datagrams.push(Datagram {
                cmd: Cmd::WriteModulationBuffer,
                payload,
            });
            offset += u32::try_from(chunk.len()).expect("chunk fits one frame");
        }
        Ok(datagrams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_modulation_buffer_single_frame() {
        let ds = WriteModulationBuffer {
            bank: Bank::B1,
            offset: 0x0102,
            data: vec![0xAA, 0xBB, 0xCC],
        }
        .datagrams()
        .unwrap();

        assert_eq!(ds.len(), 1);
        let p = &ds[0].payload;
        assert_eq!(ds[0].cmd, Cmd::WriteModulationBuffer);
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
        let ds = WriteModulationBuffer {
            bank: Bank::B0,
            offset: 100,
            data: data.clone(),
        }
        .datagrams()
        .unwrap();

        assert_eq!(ds.len(), 2);
        assert_eq!(WRITE_MAX_DATA_LEN % 2, 0, "split must keep offsets even");

        let p0 = &ds[0].payload;
        assert_eq!(&p0[2..6], &100u32.to_le_bytes());
        let max = u16::try_from(WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p0[6..8], &max.to_le_bytes());
        assert_eq!(
            &p0[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + WRITE_MAX_DATA_LEN],
            &data[..WRITE_MAX_DATA_LEN]
        );

        let p1 = &ds[1].payload;
        assert_eq!(&p1[2..6], &(100 + u32::from(max)).to_le_bytes());
        let rest = u16::try_from(1000 - WRITE_MAX_DATA_LEN).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
    }

    #[test]
    fn write_modulation_buffer_accepts_exactly_full_capacity() {
        let ds = WriteModulationBuffer {
            bank: Bank::B0,
            offset: 0,
            data: vec![0x55; MOD_BUFFER_SAMPLES],
        }
        .datagrams()
        .unwrap();
        assert_eq!(ds.len(), MOD_BUFFER_SAMPLES.div_ceil(WRITE_MAX_DATA_LEN));
    }

    #[test]
    fn write_modulation_buffer_rejects_invalid_inputs() {
        let base = |offset: u32, data: Vec<u8>| WriteModulationBuffer {
            bank: Bank::B0,
            offset,
            data,
        };
        assert!(matches!(
            base(0, vec![]).datagrams(),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            base(1, vec![0; 2]).datagrams(),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            base(u32::try_from(MOD_BUFFER_SAMPLES - 2).unwrap(), vec![0; 3]).datagrams(),
            Err(Error::InvalidPayload(_))
        ));
    }
}

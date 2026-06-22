use crate::error::{Error, PayloadError};
use crate::params::{FOCUS_WORDS, MAX_FOCI_TOTAL};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Focus, PatternBank};

use super::{Distribution, MAX_FOCI_PER_FRAME, Operation, WRITE_HEADER_BYTES};

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
pub struct WriteFociBuffer {
    pub bank: PatternBank,
    pub offset: u32,
    pub foci: Vec<Focus>,
}

impl Operation for WriteFociBuffer {
    fn frames(&self) -> usize {
        self.foci.len().div_ceil(MAX_FOCI_PER_FRAME).max(1)
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
        if self.foci.is_empty() {
            return Err(Error::InvalidPayload(PayloadError::FociEmpty));
        }
        let end = self.offset as usize + self.foci.len();
        if end > MAX_FOCI_TOTAL {
            return Err(Error::InvalidPayload(
                PayloadError::FociWriteExceedsCapacity {
                    offset: self.offset as usize,
                    end,
                    capacity: MAX_FOCI_TOTAL,
                },
            ));
        }

        let start = frame * MAX_FOCI_PER_FRAME;
        let chunk = &self.foci[start..(start + MAX_FOCI_PER_FRAME).min(self.foci.len())];
        let word_offset = u32::try_from((self.offset as usize + start) * FOCUS_WORDS)
            .expect("bounded by capacity");
        let len = u16::try_from(chunk.len() * FOCUS_WORDS * 2).expect("bounded by frame");

        let (header, _) =
            WriteHeader::mut_from_prefix(&mut out[..]).expect("WriteHeader fits the payload");
        header.bank = self.bank.as_u8();
        header.offset = word_offset.into();
        header.len = len.into();
        for (dst, focus) in out[WRITE_HEADER_BYTES..].chunks_exact_mut(8).zip(chunk) {
            dst.copy_from_slice(&focus.encode()?.to_le_bytes());
        }
        Ok(Cmd::WritePatternBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::FOCUS_COORD_MAX;

    fn encode(op: &WriteFociBuffer, frame: usize) -> Result<[u8; PAYLOAD_BYTES], Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(0, frame, &mut out)?;
        Ok(out)
    }

    #[test]
    fn write_foci_buffer_packs_and_splits() {
        let foci: Vec<Focus> = (0..100)
            .map(|i| Focus {
                x: i,
                y: -i,
                z: 1000 + i,
                intensity_or_offset: 0xFF,
            })
            .collect();
        let op = WriteFociBuffer {
            bank: PatternBank::B0,
            offset: 10,
            foci: foci.clone(),
        };

        assert_eq!(op.frames(), 2, "100 foci > 77 per frame");

        let p0 = encode(&op, 0).unwrap();
        let word_offset0 = u32::try_from(10 * FOCUS_WORDS).unwrap();
        assert_eq!(&p0[2..6], &word_offset0.to_le_bytes());
        let len0 = u16::try_from(MAX_FOCI_PER_FRAME * 8).unwrap();
        assert_eq!(&p0[6..8], &len0.to_le_bytes());
        let first = u64::from_le_bytes(
            p0[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(first, foci[0].encode().unwrap());

        let p1 = encode(&op, 1).unwrap();
        let word_offset1 = u32::try_from((10 + MAX_FOCI_PER_FRAME) * FOCUS_WORDS).unwrap();
        assert_eq!(&p1[2..6], &word_offset1.to_le_bytes());
        let rest = u16::try_from((100 - MAX_FOCI_PER_FRAME) * 8).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_inputs() {
        let base = |foci: Vec<Focus>, offset: u32| WriteFociBuffer {
            bank: PatternBank::B0,
            offset,
            foci,
        };
        assert!(matches!(
            encode(&base(vec![], 0), 0),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(
                &base(
                    vec![Focus {
                        x: FOCUS_COORD_MAX + 1,
                        ..Focus::default()
                    }],
                    0
                ),
                0
            ),
            Err(Error::InvalidPayload(_))
        ));
        assert!(matches!(
            encode(
                &base(
                    vec![Focus::default(); 2],
                    u32::try_from(MAX_FOCI_TOTAL - 1).unwrap()
                ),
                0
            ),
            Err(Error::InvalidPayload(_))
        ));
    }
}

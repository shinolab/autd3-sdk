use crate::error::{Error, PayloadError};
use crate::params::{FOCUS_WORDS, MAX_FOCI_TOTAL};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ControlPoints, PatternBank};

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
pub struct WriteFociBuffer<'a, const N: usize> {
    pub bank: PatternBank,
    pub index_offset: usize,
    pub points: &'a [ControlPoints<N>],
}

impl<const N: usize> Operation for WriteFociBuffer<'_, N> {
    fn frames(&self) -> usize {
        (self.points.len() * N).div_ceil(MAX_FOCI_PER_FRAME).max(1)
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
        let total = self.points.len() * N;
        if total == 0 {
            return Err(Error::InvalidPayload(PayloadError::FociEmpty));
        }
        let base = self.index_offset * N;
        let end = base + total;
        if end > MAX_FOCI_TOTAL {
            return Err(Error::InvalidPayload(
                PayloadError::FociWriteExceedsCapacity {
                    offset: base,
                    end,
                    capacity: MAX_FOCI_TOTAL,
                },
            ));
        }

        let start = frame * MAX_FOCI_PER_FRAME;
        let chunk_len = (start + MAX_FOCI_PER_FRAME).min(total) - start;
        let word_offset = u32::try_from((base + start) * FOCUS_WORDS).expect("bounded by capacity");
        let len = u16::try_from(chunk_len * FOCUS_WORDS * 2).expect("bounded by frame");

        let (header, _) =
            WriteHeader::mut_from_prefix(&mut out[..]).expect("WriteHeader fits the payload");
        header.bank = self.bank.as_u8();
        header.offset = word_offset.into();
        header.len = len.into();
        for (dst, k) in out[WRITE_HEADER_BYTES..]
            .chunks_exact_mut(8)
            .zip(start..start + chunk_len)
        {
            let focus = self.points[k / N].focus(k % N);
            dst.copy_from_slice(&focus.encode()?.to_le_bytes());
        }
        Ok(Cmd::WritePatternBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;

    fn encode<const N: usize>(
        op: &WriteFociBuffer<'_, N>,
        frame: usize,
    ) -> Result<[u8; PAYLOAD_BYTES], Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(0, frame, &mut out)?;
        Ok(out)
    }

    #[test]
    fn write_foci_buffer_packs_and_splits() {
        let points: Vec<ControlPoints<1>> = (0..100)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let op = WriteFociBuffer {
            bank: PatternBank::B0,
            index_offset: 10,
            points: &points,
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
        assert_eq!(first, points[0].focus(0).encode().unwrap());

        let p1 = encode(&op, 1).unwrap();
        let word_offset1 = u32::try_from((10 + MAX_FOCI_PER_FRAME) * FOCUS_WORDS).unwrap();
        assert_eq!(&p1[2..6], &word_offset1.to_le_bytes());
        let rest = u16::try_from((100 - MAX_FOCI_PER_FRAME) * 8).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_inputs() {
        let empty: [ControlPoints<1>; 0] = [];
        assert!(matches!(
            encode(
                &WriteFociBuffer {
                    bank: PatternBank::B0,
                    index_offset: 0,
                    points: &empty,
                },
                0
            ),
            Err(Error::InvalidPayload(_))
        ));

        let out_of_range = [ControlPoints::from(Point3::new(1.0e9, 0.0, 0.0))];
        assert!(matches!(
            encode(
                &WriteFociBuffer {
                    bank: PatternBank::B0,
                    index_offset: 0,
                    points: &out_of_range,
                },
                0
            ),
            Err(Error::InvalidPayload(_))
        ));

        let two = [ControlPoints::from(Point3::origin()); 2];
        assert!(matches!(
            encode(
                &WriteFociBuffer {
                    bank: PatternBank::B0,
                    index_offset: MAX_FOCI_TOTAL - 1,
                    points: &two,
                },
                0
            ),
            Err(Error::InvalidPayload(_))
        ));
    }
}

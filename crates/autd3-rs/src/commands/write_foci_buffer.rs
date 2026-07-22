use crate::datagram::DatagramBuilder;
use crate::error::PayloadError;
use crate::value::{ControlPoints, PatternBank};

use super::Command;
use super::operation::{MAX_FOCI_PER_FRAME, WriteFociChunk};

#[derive(Clone, Debug)]
pub struct WriteFociBuffer<'a, const N: usize> {
    pub bank: PatternBank,
    pub index_offset: usize,
    pub points: &'a [ControlPoints<N>],
}

impl<'a, const N: usize> Command<'a> for WriteFociBuffer<'a, N> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        let total = self.points.len() * N;
        if total == 0 {
            builder.reject(PayloadError::FociEmpty);
            return;
        }
        let mut start = 0;
        while start < total {
            let focus_len = MAX_FOCI_PER_FRAME.min(total - start);
            builder.push(WriteFociChunk {
                bank: self.bank,
                index_offset: self.index_offset,
                points: self.points,
                focus_start: start,
                focus_len,
            });
            start += focus_len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datagram::Frames;
    use crate::error::Error;
    use crate::geometry::Point3;
    use crate::params::{FOCUS_WORDS, MAX_FOCI_TOTAL};
    use crate::protocol::PAYLOAD_BYTES;
    use crate::test_utils::{test_device, test_geometry_arc};
    use autd3_cpu_wire::payload::WritePatternPayload;
    const HEADER_BYTES: usize = core::mem::size_of::<WritePatternPayload>();

    fn expand<const N: usize>(op: WriteFociBuffer<'_, N>) -> Result<Frames, Error> {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(op);
        b.build()
    }

    fn payload(frames: &Frames, index: usize) -> [u8; PAYLOAD_BYTES] {
        frames.frame(index).unwrap().datagrams()[0].payload
    }

    #[test]
    fn write_foci_buffer_packs_and_splits() {
        let points: Vec<ControlPoints<1>> = (0..100)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let frames = expand(WriteFociBuffer {
            bank: PatternBank::B0,
            index_offset: 10,
            points: &points,
        })
        .unwrap();

        assert_eq!(frames.len(), 2, "100 foci > 77 per frame");

        let p0 = payload(&frames, 0);
        let word_offset0 = u32::try_from(10 * FOCUS_WORDS).unwrap();
        assert_eq!(&p0[2..6], &word_offset0.to_le_bytes());
        let len0 = u16::try_from(MAX_FOCI_PER_FRAME * 8).unwrap();
        assert_eq!(&p0[6..8], &len0.to_le_bytes());
        let first = u64::from_le_bytes(p0[HEADER_BYTES..HEADER_BYTES + 8].try_into().unwrap());
        assert_eq!(first, points[0].focus(&test_device(0), 0).encode().unwrap());

        let p1 = payload(&frames, 1);
        let word_offset1 = u32::try_from((10 + MAX_FOCI_PER_FRAME) * FOCUS_WORDS).unwrap();
        assert_eq!(&p1[2..6], &word_offset1.to_le_bytes());
        let rest = u16::try_from((100 - MAX_FOCI_PER_FRAME) * 8).unwrap();
        assert_eq!(&p1[6..8], &rest.to_le_bytes());
        let first_of_rest =
            u64::from_le_bytes(p1[HEADER_BYTES..HEADER_BYTES + 8].try_into().unwrap());
        assert_eq!(
            first_of_rest,
            points[MAX_FOCI_PER_FRAME]
                .focus(&test_device(0), 0)
                .encode()
                .unwrap()
        );
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_inputs() {
        let empty: [ControlPoints<1>; 0] = [];
        assert!(matches!(
            expand(WriteFociBuffer {
                bank: PatternBank::B0,
                index_offset: 0,
                points: &empty,
            }),
            Err(Error::InvalidPayload(_))
        ));

        let out_of_range = [ControlPoints::from(Point3::new(1.0e9, 0.0, 0.0))];
        assert!(matches!(
            expand(WriteFociBuffer {
                bank: PatternBank::B0,
                index_offset: 0,
                points: &out_of_range,
            }),
            Err(Error::Encode(_))
        ));

        let two = [ControlPoints::from(Point3::origin()); 2];
        assert!(matches!(
            expand(WriteFociBuffer {
                bank: PatternBank::B0,
                index_offset: MAX_FOCI_TOTAL - 1,
                points: &two,
            }),
            Err(Error::InvalidPayload(_))
        ));
    }
}

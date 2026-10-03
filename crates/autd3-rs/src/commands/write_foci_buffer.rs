use autd3_cpu_wire::payload::WriteFociPayload;

use crate::datagram::DatagramBuilder;
use crate::error::PayloadError;
use crate::params::FOCUS_WORDS;
use crate::protocol::PAYLOAD_BYTES;
use crate::value::{ControlPoints, PatternBank};

use super::Command;
use super::operation::WriteFociChunk;

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
        let max_foci_per_frame =
            (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2);
        let mut start = 0;
        while start < total {
            let focus_len = max_foci_per_frame.min(total - start);
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
    use crate::params::MAX_FOCI_TOTAL;
    use crate::test_utils::{test_device, test_geometry_arc};
    const HEADER_BYTES: usize = core::mem::size_of::<WriteFociPayload>();

    fn expand<const N: usize>(op: WriteFociBuffer<'_, N>) -> Result<Frames, Error> {
        let mut b = DatagramBuilder::new(test_geometry_arc(1));
        b.push(op);
        b.build()
    }

    fn payload(frames: &Frames, index: usize) -> Vec<u8> {
        frames.frame(index).unwrap().datagrams()[0]
            .payload()
            .to_vec()
    }

    #[test]
    fn write_foci_buffer_packs_and_splits() {
        let max_foci_per_frame =
            (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2);
        let total = max_foci_per_frame + 23;
        let points: Vec<ControlPoints<1>> = (0..total)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let frames = expand(WriteFociBuffer {
            bank: PatternBank::B0,
            index_offset: 10,
            points: &points,
        })
        .unwrap();

        assert_eq!(frames.len(), 2, "one frame more than fits");

        let p0 = payload(&frames, 0);
        let word_offset0 = u32::try_from(10 * FOCUS_WORDS).unwrap();
        assert_eq!(&p0[2..6], &word_offset0.to_le_bytes());
        assert_eq!(p0.len(), HEADER_BYTES + max_foci_per_frame * 8);
        let first = u64::from_le_bytes(p0[HEADER_BYTES..HEADER_BYTES + 8].try_into().unwrap());
        assert_eq!(first, points[0].focus(&test_device(0), 0).encode().unwrap());

        let p1 = payload(&frames, 1);
        let word_offset1 = u32::try_from((10 + max_foci_per_frame) * FOCUS_WORDS).unwrap();
        assert_eq!(&p1[2..6], &word_offset1.to_le_bytes());
        assert_eq!(p1.len(), HEADER_BYTES + (total - max_foci_per_frame) * 8);
        let first_of_rest =
            u64::from_le_bytes(p1[HEADER_BYTES..HEADER_BYTES + 8].try_into().unwrap());
        assert_eq!(
            first_of_rest,
            points[max_foci_per_frame]
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

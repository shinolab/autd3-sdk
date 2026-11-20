use crate::commands::Command;
use crate::datagram::{Datagram, Expansion};
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::{FOCUS_WORDS, NUM_FOCI_MAX};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ControlPoints, PatternBank};

use super::{Distribution, Operation, set_cmd, write_header};
use autd3_cpu_wire::payload::WriteFociPayload;

#[derive(Clone, Debug)]
pub struct WriteFociBuffer<'a> {
    pub bank: PatternBank,
    pub start_index: usize,
    pub points: &'a [ControlPoints],
}

pub(crate) fn num_foci(points: &[ControlPoints]) -> Result<u8, PayloadError> {
    let Some(first) = points.first() else {
        return Err(PayloadError::FociEmpty);
    };
    let expected = first.points.len();
    let num_foci = u8::try_from(expected)
        .ok()
        .filter(|n| (1..=NUM_FOCI_MAX).contains(n))
        .ok_or(PayloadError::NumFociOutOfRange { num_foci: expected })?;
    if let Some((index, got)) = points
        .iter()
        .map(|cp| cp.points.len())
        .enumerate()
        .find(|&(_, len)| len != expected)
    {
        return Err(PayloadError::NumFociMismatch {
            index,
            expected,
            got,
        });
    }
    Ok(num_foci)
}

impl Command for WriteFociBuffer<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        let num_foci = num_foci(self.points)?;
        let max_len = WriteFociChunk::max_len(num_foci);
        for (i, points) in self.points.chunks(max_len).enumerate() {
            expansion.push_op(&WriteFociChunk {
                bank: self.bank,
                start_index: self.start_index.saturating_add(i * max_len),
                num_foci,
                points,
            })?;
        }
        Ok(())
    }
}

struct WriteFociChunk<'a> {
    bank: PatternBank,
    start_index: usize,
    num_foci: u8,
    points: &'a [ControlPoints],
}

impl WriteFociChunk<'_> {
    const fn max_len(num_foci: u8) -> usize {
        (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2) / num_foci as usize
    }
}

impl crate::sealed::Sealed for WriteFociChunk<'_> {}

impl Operation for WriteFociChunk<'_> {
    fn waits_for_previous(&self) -> bool {
        false
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let n = usize::from(self.num_foci);
        let total = self.points.len() * n;
        let header = WriteFociPayload::new(self.bank, self.start_index.saturating_mul(n), total)?;
        let rest = write_header(out, &header);
        let foci = self
            .points
            .iter()
            .flat_map(|points| (0..n).map(move |j| points.focus(device, j)));
        for (dst, focus) in rest.as_chunks_mut::<8>().0.iter_mut().zip(foci) {
            *dst = focus.encode()?.to_le_bytes();
        }
        set_cmd(
            out,
            Cmd::WriteFociBuffer,
            size_of::<WriteFociPayload>() + total * FOCUS_WORDS * 2,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datagram::Frames;
    use crate::geometry::Point3;
    use crate::params::MAX_FOCI_TOTAL;
    use crate::test_utils::{build, cmds, payload, test_device};
    use crate::value::{ControlPoint, Intensity, Phase};

    fn write(start_index: usize, points: &[ControlPoints]) -> Result<Frames, Error> {
        build(
            1,
            WriteFociBuffer {
                bank: PatternBank::B1,
                start_index,
                points,
            },
        )
    }

    fn with(n: usize) -> ControlPoints {
        ControlPoints::new(
            vec![ControlPoint::from(Point3::origin()); n],
            Intensity::MAX,
        )
    }

    #[test]
    fn write_foci_buffer_writes_foci_at_the_start_index() {
        let points: Vec<ControlPoints> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();

        let frames = write(10, &points).unwrap();

        assert_eq!(cmds(&frames), [Cmd::WriteFociBuffer]);
        assert_eq!(
            frames.frame(0).unwrap().distribution(),
            Distribution::PerDevice
        );
        let out = payload(&frames, 0, 0);
        assert_eq!(out.len(), size_of::<WriteFociPayload>() + 4 * 8);
        assert_eq!(out[0], 1, "bank B1");
        let word_offset = u32::try_from(10 * FOCUS_WORDS).unwrap();
        assert_eq!(&out[1..5], &word_offset.to_le_bytes());
        let data = &out[size_of::<WriteFociPayload>()..];
        for (i, point) in points.iter().enumerate() {
            let focus = u64::from_le_bytes(data[i * 8..][..8].try_into().unwrap());
            assert_eq!(focus, point.focus(&test_device(0), 0).encode().unwrap());
        }
    }

    #[test]
    fn write_foci_buffer_packs_every_focus_of_a_control_point_in_order() {
        let points = [
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(0.0, 0.0, 1.0), Phase::ZERO),
                    ControlPoint::new(Point3::new(0.0, 0.0, 2.0), Phase(0x10)),
                ],
                Intensity(0xAA),
            ),
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(0.0, 0.0, 3.0), Phase::ZERO),
                    ControlPoint::new(Point3::new(0.0, 0.0, 4.0), Phase(0x20)),
                ],
                Intensity(0xBB),
            ),
        ];

        let frames = write(3, &points).unwrap();

        let out = payload(&frames, 0, 0);
        assert_eq!(out.len(), size_of::<WriteFociPayload>() + 4 * 8);
        let word_offset = u32::try_from(3 * 2 * FOCUS_WORDS).unwrap();
        assert_eq!(&out[1..5], &word_offset.to_le_bytes());
        let data = &out[size_of::<WriteFociPayload>()..];
        let expected = [(0, 0), (0, 1), (1, 0), (1, 1)];
        for (k, (i, j)) in expected.into_iter().enumerate() {
            let focus = u64::from_le_bytes(data[k * 8..][..8].try_into().unwrap());
            assert_eq!(focus, points[i].focus(&test_device(0), j).encode().unwrap());
        }
    }

    #[test]
    fn write_foci_buffer_splits_on_control_point_boundaries_from_the_start_index() {
        let max = WriteFociChunk::max_len(7);
        let points = vec![with(7); max + 1];

        let frames = write(5, &points).unwrap();

        assert_eq!(cmds(&frames), [Cmd::WriteFociBuffer, Cmd::WriteFociBuffer]);
        let first = payload(&frames, 0, 0);
        assert_eq!(
            first.len(),
            size_of::<WriteFociPayload>() + max * 7 * FOCUS_WORDS * 2
        );
        assert_eq!(
            &first[1..5],
            &u32::try_from(5 * 7 * FOCUS_WORDS).unwrap().to_le_bytes()
        );
        let second = payload(&frames, 1, 0);
        assert_eq!(
            &second[1..5],
            &u32::try_from((5 + max) * 7 * FOCUS_WORDS)
                .unwrap()
                .to_le_bytes()
        );
        assert_eq!(
            second.len(),
            size_of::<WriteFociPayload>() + 7 * FOCUS_WORDS * 2
        );
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_inputs() {
        assert!(matches!(
            write(0, &[]),
            Err(Error::InvalidPayload(PayloadError::FociEmpty))
        ));
        assert!(matches!(
            write(0, &[ControlPoints::from(Point3::new(1.0e9, 0.0, 0.0))]),
            Err(Error::InvalidPayload(PayloadError::Focus(_)))
        ));
        assert!(matches!(
            write(0, &[ControlPoints::from(Point3::new(0.0, 0.0, f32::NAN))]),
            Err(Error::InvalidPayload(PayloadError::Focus(_)))
        ));
        assert!(matches!(
            write(
                MAX_FOCI_TOTAL - 1,
                &[
                    ControlPoints::from(Point3::origin()),
                    ControlPoints::from(Point3::origin())
                ]
            ),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_num_foci() {
        assert!(matches!(
            write(0, &[with(0)]),
            Err(Error::InvalidPayload(PayloadError::NumFociOutOfRange {
                num_foci: 0
            }))
        ));
        let too_many = usize::from(NUM_FOCI_MAX) + 1;
        assert!(matches!(
            write(0, &[with(too_many)]),
            Err(Error::InvalidPayload(PayloadError::NumFociOutOfRange { num_foci }))
                if num_foci == too_many
        ));
        assert!(matches!(
            write(0, &[with(2), with(2), with(3)]),
            Err(Error::InvalidPayload(PayloadError::NumFociMismatch {
                index: 2,
                expected: 2,
                got: 3
            }))
        ));
    }

    #[test]
    fn write_foci_buffer_rejects_a_num_foci_mismatch_in_a_later_frame() {
        let max = WriteFociChunk::max_len(2);
        let mut points = vec![with(2); max + 1];
        points[max] = with(1);
        assert!(matches!(
            write(0, &points),
            Err(Error::InvalidPayload(PayloadError::NumFociMismatch { index, expected: 2, got: 1 }))
                if index == max
        ));
    }

    #[test]
    fn write_foci_buffer_converts_to_device_local_coordinates() {
        use crate::geometry::{Autd3, UnitQuaternion, Vector3};

        let rot = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), core::f32::consts::FRAC_PI_2);
        let device: Device = Autd3::new(Point3::new(10.0, 20.0, 30.0), rot).into();
        let global = Point3::from(device.position(0).coords + rot * Vector3::new(1.0, 2.0, 3.0));
        let points = [ControlPoints::from(global)];
        let op = WriteFociChunk {
            bank: PatternBank::B0,
            start_index: 0,
            num_foci: 1,
            points: &points,
        };

        let mut out = Datagram::no_payload(Cmd::Nop);
        op.encode(&device, &mut out).unwrap();
        let f = u64::from_le_bytes(
            out.payload[size_of::<WriteFociPayload>()..][..8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(f & 0x3_FFFF, 40);
        assert_eq!((f >> 18) & 0x3_FFFF, 80);
        assert_eq!((f >> 36) & 0x3_FFFF, 120);
    }
}

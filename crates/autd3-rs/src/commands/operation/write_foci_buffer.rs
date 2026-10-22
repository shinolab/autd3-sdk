use crate::datagram::Datagram;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::{FOCUS_WORDS, NUM_FOCI_MAX};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ControlPoints, PatternBank};

use super::{Distribution, Operation, set_cmd, write_header};
use autd3_cpu_wire::payload::WriteFociPayload;

#[derive(Clone, Copy, Debug)]
pub struct WriteFociBuffer<'a> {
    pub bank: PatternBank,
    pub start_index: usize,
    pub points: &'a [ControlPoints],
}

impl WriteFociBuffer<'_> {
    #[must_use]
    pub const fn max_len(num_foci: u8) -> usize {
        if num_foci == 0 || num_foci > NUM_FOCI_MAX {
            return 0;
        }
        (PAYLOAD_BYTES - size_of::<WriteFociPayload>()) / (FOCUS_WORDS * 2) / num_foci as usize
    }
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

impl crate::sealed::Sealed for WriteFociBuffer<'_> {}

impl Operation for WriteFociBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let num_foci = num_foci(self.points)?;
        let max = Self::max_len(num_foci);
        if self.points.len() > max {
            return Err(PayloadError::FociWriteTooLong {
                len: self.points.len(),
                max,
            }
            .into());
        }
        let n = usize::from(num_foci);
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
    use crate::geometry::Point3;
    use crate::params::MAX_FOCI_TOTAL;
    use crate::test_utils::{encode, test_device};
    use crate::value::{ControlPoint, Intensity, Phase};

    #[test]
    fn write_foci_buffer_writes_foci_at_the_start_index() {
        let points: Vec<ControlPoints> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let op = WriteFociBuffer {
            bank: PatternBank::B1,
            start_index: 10,
            points: &points,
        };

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            (Cmd::WriteFociBuffer, size_of::<WriteFociPayload>() + 4 * 8)
        );
        assert_eq!(out[0], 1, "bank B1");
        let word_offset = u32::try_from(10 * FOCUS_WORDS).unwrap();
        assert_eq!(&out[2..6], &word_offset.to_le_bytes());
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

        let (cmd, out) = encode(&WriteFociBuffer {
            bank: PatternBank::B0,
            start_index: 3,
            points: &points,
        })
        .unwrap();

        assert_eq!(cmd.1, size_of::<WriteFociPayload>() + 4 * 8);
        let word_offset = u32::try_from(3 * 2 * FOCUS_WORDS).unwrap();
        assert_eq!(&out[2..6], &word_offset.to_le_bytes());
        let data = &out[size_of::<WriteFociPayload>()..];
        let expected = [(0, 0), (0, 1), (1, 0), (1, 1)];
        for (k, (i, j)) in expected.into_iter().enumerate() {
            let focus = u64::from_le_bytes(data[k * 8..][..8].try_into().unwrap());
            assert_eq!(focus, points[i].focus(&test_device(0), j).encode().unwrap());
        }
    }

    #[test]
    fn write_foci_buffer_accepts_exactly_one_frame_and_rejects_more() {
        let points =
            vec![
                ControlPoints::new([ControlPoint::from(Point3::origin()); 3], Intensity::MAX);
                WriteFociBuffer::max_len(3) + 1
            ];
        let write = |points| {
            encode(&WriteFociBuffer {
                bank: PatternBank::B0,
                start_index: 0,
                points,
            })
        };

        assert!(write(&points[..WriteFociBuffer::max_len(3)]).is_ok());
        assert!(matches!(
            write(&points),
            Err(Error::InvalidPayload(PayloadError::FociWriteTooLong { len, max }))
                if len == WriteFociBuffer::max_len(3) + 1 && max == WriteFociBuffer::max_len(3)
        ));
    }

    #[test]
    fn write_foci_buffer_rejects_invalid_inputs() {
        let write = |start_index, points: &[ControlPoints]| {
            encode(&WriteFociBuffer {
                bank: PatternBank::B0,
                start_index,
                points,
            })
        };

        assert!(matches!(
            write(0, &[]),
            Err(Error::InvalidPayload(PayloadError::FociEmpty))
        ));
        assert!(matches!(
            write(0, &[ControlPoints::from(Point3::new(1.0e9, 0.0, 0.0))]),
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
        let write = |points: &[ControlPoints]| {
            encode(&WriteFociBuffer {
                bank: PatternBank::B0,
                start_index: 0,
                points,
            })
        };
        let with = |n: usize| {
            ControlPoints::new(
                vec![ControlPoint::from(Point3::origin()); n],
                Intensity::MAX,
            )
        };

        assert!(matches!(
            write(&[with(0)]),
            Err(Error::InvalidPayload(PayloadError::NumFociOutOfRange {
                num_foci: 0
            }))
        ));
        let too_many = usize::from(NUM_FOCI_MAX) + 1;
        assert!(matches!(
            write(&[with(too_many)]),
            Err(Error::InvalidPayload(PayloadError::NumFociOutOfRange { num_foci }))
                if num_foci == too_many
        ));
        assert!(matches!(
            write(&[with(2), with(2), with(3)]),
            Err(Error::InvalidPayload(PayloadError::NumFociMismatch {
                index: 2,
                expected: 2,
                got: 3
            }))
        ));
    }

    #[test]
    fn write_foci_buffer_max_len_is_zero_outside_the_num_foci_range() {
        assert_eq!(WriteFociBuffer::max_len(0), 0);
        assert_eq!(WriteFociBuffer::max_len(NUM_FOCI_MAX + 1), 0);
        assert!(WriteFociBuffer::max_len(NUM_FOCI_MAX) > 0);
    }

    #[test]
    fn write_foci_buffer_converts_to_device_local_coordinates() {
        use crate::geometry::{Autd3, UnitQuaternion, Vector3};

        let rot = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), core::f32::consts::FRAC_PI_2);
        let device: Device = Autd3::new(Point3::new(10.0, 20.0, 30.0), rot).into();
        let global = Point3::from(device.position(0).coords + rot * Vector3::new(1.0, 2.0, 3.0));
        let points = [ControlPoints::from(global)];
        let op = WriteFociBuffer {
            bank: PatternBank::B0,
            start_index: 0,
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

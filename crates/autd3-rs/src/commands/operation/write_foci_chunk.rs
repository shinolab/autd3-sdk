use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::{FOCUS_WORDS, MAX_FOCI_TOTAL};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ControlPoints, PatternBank};

use super::{Distribution, Operation};
use autd3_cpu_wire::payload::WritePatternPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::{U16, U32};

#[derive(Clone, Debug)]
pub(crate) struct WriteFociChunk<'a, const N: usize> {
    pub bank: PatternBank,
    pub index_offset: usize,
    pub points: &'a [ControlPoints<N>],
    pub focus_start: usize,
    pub focus_len: usize,
}

impl<const N: usize> Operation for WriteFociChunk<'_, N> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let total = self.points.len() * N;
        if total == 0 {
            return Err(PayloadError::FociEmpty.into());
        }
        let base = self.index_offset * N;
        let end = base + total;
        if end > MAX_FOCI_TOTAL {
            return Err(PayloadError::FociWriteExceedsCapacity {
                offset: base,
                end,
                capacity: MAX_FOCI_TOTAL,
            }
            .into());
        }

        let start = self.focus_start;
        let word_offset = u32::try_from((base + start) * FOCUS_WORDS).expect("bounded by capacity");
        let len = u16::try_from(self.focus_len * FOCUS_WORDS * 2).expect("bounded by frame");

        let (h, rest) = WritePatternPayload::mut_from_prefix(&mut out[..]).unwrap();
        *h = WritePatternPayload {
            bank: self.bank.as_u8(),
            reserved: 0,
            offset: U32::new(word_offset),
            data_len: U16::new(len),
        };
        for (dst, k) in rest.chunks_exact_mut(8).zip(start..start + self.focus_len) {
            let focus = self.points[k / N].focus(device, k % N);
            dst.copy_from_slice(&focus.encode()?.to_le_bytes());
        }
        Ok(Cmd::WritePatternBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;
    use crate::test_utils::test_device;
    use autd3_cpu_wire::layout::WRITE_HEADER_BYTES;

    #[test]
    fn write_foci_chunk_writes_its_own_window() {
        let points: Vec<ControlPoints<1>> = (0..4)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32)))
            .collect();
        let op = WriteFociChunk {
            bank: PatternBank::B0,
            index_offset: 10,
            points: &points,
            focus_start: 2,
            focus_len: 2,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternBuffer);
        let word_offset = u32::try_from((10 + 2) * FOCUS_WORDS).unwrap();
        assert_eq!(&out[2..6], &word_offset.to_le_bytes());
        assert_eq!(&out[6..8], &u16::try_from(2 * 8).unwrap().to_le_bytes());
        let first = u64::from_le_bytes(
            out[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(first, points[2].focus(&test_device(0), 0).encode().unwrap());
    }

    #[test]
    fn write_foci_chunk_converts_to_device_local_coordinates() {
        use crate::geometry::{Autd3, Device, UnitQuaternion, Vector3};

        let rot = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), core::f32::consts::FRAC_PI_2);
        let device: Device = Autd3::new(Point3::new(10.0, 20.0, 30.0), rot).into();
        let global = Point3::from(device.position(0).coords + rot * Vector3::new(1.0, 2.0, 3.0));
        let points = [ControlPoints::from(global)];
        let op = WriteFociChunk {
            bank: PatternBank::B0,
            index_offset: 0,
            points: &points,
            focus_start: 0,
            focus_len: 1,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(&device, &mut out).unwrap();
        let f = u64::from_le_bytes(
            out[WRITE_HEADER_BYTES..WRITE_HEADER_BYTES + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(f & 0x3_FFFF, 40);
        assert_eq!((f >> 18) & 0x3_FFFF, 80);
        assert_eq!((f >> 36) & 0x3_FFFF, 120);
    }
}

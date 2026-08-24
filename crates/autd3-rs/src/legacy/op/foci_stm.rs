use autd3_rs_core::common::Velocity;
use autd3_rs_core::geometry::Device;
use autd3_rs_core::value::{ControlPoints, LoopBehavior, SamplingConfig};
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::params::{
    FOCI_STM_BUF_SIZE_MAX, FOCI_STM_FLAG_BEGIN, FOCI_STM_FLAG_END, FOCI_STM_FLAG_TRANSITION,
    FOCI_STM_FOCI_NUM_MAX, FOCI_STM_FOCI_NUM_MIN, STM_BUF_SIZE_MIN,
};
use crate::legacy::wire::{Segment, Tag, TransitionMode};

const FOCUS_BYTES: usize = size_of::<u64>();

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct FociStmHead {
    tag: u8,
    flag: u8,
    send_num: u8,
    segment: u8,
    transition_mode: u8,
    num_foci: u8,
    sound_speed: u16,
    freq_div: u16,
    rep: u16,
    _pad: [u8; 4],
    transition_value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct FociStmSubseq {
    tag: u8,
    flag: u8,
    send_num: u8,
    segment: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FociStmOption {
    pub segment: Segment,
    pub sound_speed: Velocity,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl Default for FociStmOption {
    fn default() -> Self {
        Self {
            segment: Segment::S0,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FociStm<'a, const N: usize> {
    points: &'a [ControlPoints<N>],
    config: SamplingConfig,
    option: FociStmOption,
    sent: usize,
}

impl<'a, const N: usize> FociStm<'a, N> {
    #[must_use]
    pub fn new(
        config: impl Into<SamplingConfig>,
        points: &'a [ControlPoints<N>],
        option: FociStmOption,
    ) -> Self {
        Self {
            points,
            config: config.into(),
            option,
            sent: 0,
        }
    }

    fn validate(&self) -> Result<(), PayloadError> {
        if !(FOCI_STM_FOCI_NUM_MIN..=FOCI_STM_FOCI_NUM_MAX).contains(&N) {
            return Err(PayloadError::NumFociOutOfRange {
                num_foci: N,
                min: FOCI_STM_FOCI_NUM_MIN,
                max: FOCI_STM_FOCI_NUM_MAX,
            });
        }
        let total = self.points.len() * N;
        if !(STM_BUF_SIZE_MIN..=FOCI_STM_BUF_SIZE_MAX).contains(&total) {
            return Err(PayloadError::FociStmTotalSizeOutOfRange {
                total,
                min: STM_BUF_SIZE_MIN,
                max: FOCI_STM_BUF_SIZE_MAX,
            });
        }
        Ok(())
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn sound_speed_wire(sound_speed: Velocity) -> u16 {
    (sound_speed.m_s() * 64.0).round() as u16
}

impl<const N: usize> LegacyOperation for FociStm<'_, N> {
    fn required_size(&self, _device: &Device) -> usize {
        let head = if self.sent == 0 {
            size_of::<FociStmHead>()
        } else {
            size_of::<FociStmSubseq>()
        };
        head + FOCUS_BYTES * N
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        self.validate()?;

        let is_first = self.sent == 0;
        let offset = if is_first {
            size_of::<FociStmHead>()
        } else {
            size_of::<FociStmSubseq>()
        };
        let max_send_num = (tx.len() - offset) / (FOCUS_BYTES * N);
        let send_num = (self.points.len() - self.sent).min(max_send_num);

        let mut idx = offset;
        for points in &self.points[self.sent..self.sent + send_num] {
            for j in 0..N {
                let encoded = points.focus(device, j).encode()?;
                tx[idx..idx + FOCUS_BYTES].copy_from_slice(&encoded.to_le_bytes());
                idx += FOCUS_BYTES;
            }
        }
        self.sent += send_num;

        let mut flag = 0u8;
        if self.sent == self.points.len() {
            flag |= FOCI_STM_FLAG_END;
            if !self.option.transition_mode.is_later() {
                flag |= FOCI_STM_FLAG_TRANSITION;
            }
        }

        let send_num_wire = u8::try_from(send_num).expect("a chunk holds at most 77 patterns");
        if is_first {
            let head = FociStmHead {
                tag: Tag::FociStm.as_u8(),
                flag: flag | FOCI_STM_FLAG_BEGIN,
                send_num: send_num_wire,
                segment: self.option.segment.as_u8(),
                transition_mode: self.option.transition_mode.as_u8(),
                num_foci: u8::try_from(N).expect("num_foci is at most 8"),
                sound_speed: sound_speed_wire(self.option.sound_speed),
                freq_div: self.config.divide()?,
                rep: self.option.loop_behavior.rep(),
                _pad: [0; 4],
                transition_value: self.option.transition_mode.value(),
            };
            tx[..size_of::<FociStmHead>()].copy_from_slice(head.as_bytes());
        } else {
            let subseq = FociStmSubseq {
                tag: Tag::FociStm.as_u8(),
                flag,
                send_num: send_num_wire,
                segment: self.option.segment.as_u8(),
            };
            tx[..size_of::<FociStmSubseq>()].copy_from_slice(subseq.as_bytes());
        }
        Ok(offset + FOCUS_BYTES * send_num * N)
    }

    fn is_done(&self) -> bool {
        self.sent == self.points.len()
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU16;

    use autd3_rs_core::geometry::{Autd3, Geometry, Point3};
    use autd3_rs_core::value::{ControlPoint, Intensity, Phase};

    use super::*;
    use crate::legacy::op::test_frames;
    use crate::legacy::wire::PAYLOAD_BYTES;

    fn geometry(n: usize) -> Geometry {
        Geometry::new((0..n).map(|_| Autd3::default()).collect())
    }

    fn config() -> SamplingConfig {
        SamplingConfig::new(NonZeroU16::new(0x1234).unwrap())
    }

    fn single(z: f32) -> ControlPoints<1> {
        ControlPoints::new(
            [ControlPoint::new(Point3::new(0.0, 0.0, z), Phase::ZERO)],
            Intensity(0xAB),
        )
    }

    #[test]
    fn head_carries_all_stm_settings() {
        let geo = geometry(1);
        let points = (0..4).map(|i| single(100.0 + i as f32)).collect::<Vec<_>>();
        let mut op = FociStm::new(config(), &points, FociStmOption::default());
        assert_eq!(op.required_size(&geo[0]), 24 + 8);

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert!(op.is_done());
        assert_eq!(size, 24 + 8 * 4);

        assert_eq!(tx[0], Tag::FociStm.as_u8());
        assert_eq!(
            tx[1],
            FOCI_STM_FLAG_BEGIN | FOCI_STM_FLAG_END | FOCI_STM_FLAG_TRANSITION
        );
        assert_eq!(tx[2], 4);
        assert_eq!(tx[3], Segment::S0.as_u8());
        assert_eq!(tx[4], TransitionMode::Immediate.as_u8());
        assert_eq!(tx[5], 1);
        assert_eq!(&tx[6..8], &(340u16 * 64).to_le_bytes());
        assert_eq!(&tx[8..10], &0x1234u16.to_le_bytes());
        assert_eq!(&tx[10..12], &0xFFFFu16.to_le_bytes());
        assert_eq!(&tx[12..16], &[0u8; 4]);
        assert_eq!(&tx[16..24], &0u64.to_le_bytes());
    }

    #[test]
    fn each_focus_is_the_18_bit_packed_local_coordinate() {
        let geo = geometry(1);
        let points = vec![single(150.0), single(151.0)];
        let mut op = FociStm::new(config(), &points, FociStmOption::default());
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();

        for (i, &chunk) in tx[24..24 + 16].as_chunks::<8>().0.iter().enumerate() {
            let expected = points[i].focus(&geo[0], 0).encode().unwrap();
            assert_eq!(u64::from_le_bytes(chunk), expected);
            assert_eq!((expected >> 54) & 0xFF, 0xAB);
        }
    }

    #[test]
    fn multi_foci_writes_n_words_per_pattern() {
        let geo = geometry(1);
        let points = vec![
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(0.0, 0.0, 150.0), Phase(0x10)),
                    ControlPoint::new(Point3::new(1.0, 0.0, 150.0), Phase(0x30)),
                ],
                Intensity(0x7F),
            ),
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(0.0, 0.0, 151.0), Phase::ZERO),
                    ControlPoint::new(Point3::new(1.0, 0.0, 151.0), Phase::ZERO),
                ],
                Intensity(0x7F),
            ),
        ];
        let mut op = FociStm::new(config(), &points, FociStmOption::default());
        assert_eq!(op.required_size(&geo[0]), 24 + 16);

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let size = op.pack(&geo[0], &mut tx).unwrap();
        assert_eq!(size, 24 + 8 * 2 * 2);
        assert_eq!(tx[5], 2);

        let first = u64::from_le_bytes(tx[24..32].try_into().unwrap());
        let second = u64::from_le_bytes(tx[32..40].try_into().unwrap());
        assert_eq!((first >> 54) & 0xFF, 0x7F, "slot 0 carries the intensity");
        assert_eq!(
            (second >> 54) & 0xFF,
            0x20,
            "slot 1 carries the phase offset"
        );
    }

    #[test]
    fn sound_speed_is_scaled_by_64() {
        assert_eq!(sound_speed_wire(Velocity::from_m_s(340.0)), 21760);
        assert_eq!(sound_speed_wire(Velocity::from_m_s(343.5)), 21984);
    }

    #[test]
    fn multi_frame_split_covers_every_pattern_once() {
        let geo = geometry(1);
        let points = (0..200)
            .map(|i| single(100.0 + i as f32))
            .collect::<Vec<_>>();
        let frames = test_frames(
            &geo,
            FociStm::new(config(), &points, FociStmOption::default()),
        )
        .unwrap();
        assert!(frames.len() > 1);

        let mut restored = Vec::new();
        for round in 0..frames.len() {
            let frame = frames.frame(round).unwrap();
            let payload = &frame.frames()[0].payload;
            assert_eq!(payload[0], Tag::FociStm.as_u8());
            let offset = if round == 0 { 24 } else { 4 };
            let send_num = usize::from(payload[2]);
            for &chunk in payload[offset..offset + 8 * send_num].as_chunks::<8>().0 {
                restored.push(u64::from_le_bytes(chunk));
            }
            let is_last = round == frames.len() - 1;
            assert_eq!(payload[1] & FOCI_STM_FLAG_END != 0, is_last);
        }
        assert_eq!(restored.len(), points.len());
        for (i, actual) in restored.iter().enumerate() {
            assert_eq!(*actual, points[i].focus(&geo[0], 0).encode().unwrap());
        }
    }

    #[test]
    fn fewer_than_two_patterns_is_rejected() {
        let geo = geometry(1);
        let points = vec![single(150.0)];
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let err = FociStm::new(config(), &points, FociStmOption::default())
            .pack(&geo[0], &mut tx)
            .unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::FociStmTotalSizeOutOfRange { total: 1, .. })
        ));
    }

    #[test]
    fn more_than_eight_foci_is_rejected() {
        let geo = geometry(1);
        let points = vec![
            ControlPoints::<9>::new(
                [ControlPoint::new(Point3::origin(), Phase::ZERO); 9],
                Intensity::MAX,
            );
            2
        ];
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let err = FociStm::new(config(), &points, FociStmOption::default())
            .pack(&geo[0], &mut tx)
            .unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::NumFociOutOfRange { num_foci: 9, .. })
        ));
    }

    #[test]
    fn out_of_range_coordinates_surface_an_encode_error() {
        let geo = geometry(1);
        let points = vec![single(1.0e6), single(1.0e6)];
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        let err = FociStm::new(config(), &points, FociStmOption::default())
            .pack(&geo[0], &mut tx)
            .unwrap_err();
        assert!(matches!(err, LegacyError::Encode(_)));
    }
}

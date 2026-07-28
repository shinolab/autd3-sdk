use autd3_rs_core::error::EncodeError;
use autd3_rs_core::geometry::Device;
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::LegacyError;
use crate::legacy::wire::{Segment, Tag, TransitionMode};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct SwapSegment {
    tag: u8,
    segment: u8,
}

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct SwapSegmentWithTransition {
    tag: u8,
    segment: u8,
    transition_mode: u8,
    pad: [u8; 5],
    transition_value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyChangePatternBank {
    tag: Tag,
    segment: Segment,
    transition_mode: Option<TransitionMode>,
    done: bool,
}

impl LegacyChangePatternBank {
    #[must_use]
    pub const fn gain(segment: Segment) -> Self {
        Self {
            tag: Tag::GainLegacyChangePatternBank,
            segment,
            transition_mode: None,
            done: false,
        }
    }

    #[must_use]
    pub const fn modulation(segment: Segment, transition_mode: TransitionMode) -> Self {
        Self {
            tag: Tag::ModulationLegacyChangePatternBank,
            segment,
            transition_mode: Some(transition_mode),
            done: false,
        }
    }

    #[must_use]
    pub const fn foci_stm(segment: Segment, transition_mode: TransitionMode) -> Self {
        Self {
            tag: Tag::FociStmLegacyChangePatternBank,
            segment,
            transition_mode: Some(transition_mode),
            done: false,
        }
    }

    #[must_use]
    pub const fn gain_stm(segment: Segment, transition_mode: TransitionMode) -> Self {
        Self {
            tag: Tag::GainStmLegacyChangePatternBank,
            segment,
            transition_mode: Some(transition_mode),
            done: false,
        }
    }
}

impl LegacyOperation for LegacyChangePatternBank {
    fn required_size(&self, _device: &Device) -> usize {
        if self.transition_mode.is_some() {
            size_of::<SwapSegmentWithTransition>()
        } else {
            size_of::<SwapSegment>()
        }
    }

    fn pack(&mut self, _device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        if matches!(self.transition_mode, Some(mode) if mode.is_later()) {
            return Err(EncodeError::TransitionLaterNotEncodable.into());
        }
        self.done = true;
        let bytes = if let Some(mode) = self.transition_mode {
            let msg = SwapSegmentWithTransition {
                tag: self.tag.as_u8(),
                segment: self.segment.as_u8(),
                transition_mode: mode.as_u8(),
                pad: [0; 5],
                transition_value: mode.value(),
            };
            tx[..size_of::<SwapSegmentWithTransition>()].copy_from_slice(msg.as_bytes());
            size_of::<SwapSegmentWithTransition>()
        } else {
            let msg = SwapSegment {
                tag: self.tag.as_u8(),
                segment: self.segment.as_u8(),
            };
            tx[..size_of::<SwapSegment>()].copy_from_slice(msg.as_bytes());
            size_of::<SwapSegment>()
        };
        Ok(bytes)
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::DcSysTime;

    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default()])
    }

    #[test]
    fn gain_change_segment_is_two_bytes_without_a_transition() {
        let geo = geometry();
        let mut op = LegacyChangePatternBank::gain(Segment::S1);
        assert_eq!(op.required_size(&geo[0]), 2);
        let mut tx = [0u8; 2];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 2);
        assert!(op.is_done());
        assert_eq!(tx, [Tag::GainLegacyChangePatternBank.as_u8(), 1]);
    }

    #[test]
    fn transitioning_change_segments_are_sixteen_bytes() {
        let geo = geometry();
        let time = DcSysTime::from_nanos(0x0123_4567_89AB_CDEF);
        let mode = TransitionMode::SysTime(time);
        for (tag, mut op) in [
            (
                Tag::ModulationLegacyChangePatternBank,
                LegacyChangePatternBank::modulation(Segment::S0, mode),
            ),
            (
                Tag::FociStmLegacyChangePatternBank,
                LegacyChangePatternBank::foci_stm(Segment::S1, mode),
            ),
            (
                Tag::GainStmLegacyChangePatternBank,
                LegacyChangePatternBank::gain_stm(Segment::S1, mode),
            ),
        ] {
            assert_eq!(op.required_size(&geo[0]), 16);
            let mut tx = [0xAAu8; 16];
            assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 16);
            assert_eq!(tx[0], tag.as_u8());
            assert_eq!(tx[2], mode.as_u8());
            assert_eq!(&tx[3..8], &[0u8; 5]);
            assert_eq!(&tx[8..16], &time.sys_time().to_le_bytes());
        }
    }

    #[test]
    fn a_change_segment_refuses_to_not_transition() {
        let geo = geometry();
        let mut tx = [0u8; 16];
        for mut op in [
            LegacyChangePatternBank::modulation(Segment::S1, TransitionMode::Later),
            LegacyChangePatternBank::foci_stm(Segment::S1, TransitionMode::Later),
            LegacyChangePatternBank::gain_stm(Segment::S1, TransitionMode::Later),
        ] {
            assert!(matches!(
                op.pack(&geo[0], &mut tx),
                Err(LegacyError::Encode(
                    EncodeError::TransitionLaterNotEncodable
                ))
            ));
        }
    }
}

use autd3_rs_core::value::{PatternBank, TransitionMode};

use super::LegacyCommand;
use super::adapt::{pattern_segment, transition_mode};
use crate::legacy::datagram::LegacyDatagramBuilder;
use crate::legacy::op;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Pattern,
    FociStm,
    PatternStm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyChangePatternBank {
    kind: Kind,
    bank: PatternBank,
    transition_mode: TransitionMode,
}

impl LegacyChangePatternBank {
    #[must_use]
    pub const fn pattern(bank: PatternBank) -> Self {
        Self {
            kind: Kind::Pattern,
            bank,
            transition_mode: TransitionMode::Immediate,
        }
    }

    #[must_use]
    pub const fn foci_stm(bank: PatternBank, transition_mode: TransitionMode) -> Self {
        Self {
            kind: Kind::FociStm,
            bank,
            transition_mode,
        }
    }

    #[must_use]
    pub const fn pattern_stm(bank: PatternBank, transition_mode: TransitionMode) -> Self {
        Self {
            kind: Kind::PatternStm,
            bank,
            transition_mode,
        }
    }
}

impl<'a> LegacyCommand<'a> for LegacyChangePatternBank {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let segment = pattern_segment(self.bank);
        let mode = transition_mode(self.transition_mode, builder.dc_offset_ns());
        builder.push_op(match self.kind {
            Kind::Pattern => op::LegacyChangePatternBank::gain(segment),
            Kind::FociStm => op::LegacyChangePatternBank::foci_stm(segment, mode),
            Kind::PatternStm => op::LegacyChangePatternBank::gain_stm(segment, mode),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::{DcSysTime, GpioIn};

    use super::*;
    use crate::legacy::wire::{Segment, Tag};

    fn frames(cmd: LegacyChangePatternBank) -> Vec<u8> {
        let geometry = Arc::new(Geometry::new(vec![Autd3::default()]));
        let mut builder = LegacyDatagramBuilder::new(geometry);
        builder.push(cmd);
        let frames = builder.build().unwrap();
        assert_eq!(frames.len(), 1);
        frames.frame(0).unwrap().frames()[0].payload.to_vec()
    }

    #[test]
    fn pattern_maps_onto_the_gain_tag_without_a_transition() {
        for (bank, segment) in [
            (PatternBank::B0, Segment::S0),
            (PatternBank::B1, Segment::S1),
        ] {
            let payload = frames(LegacyChangePatternBank::pattern(bank));
            assert_eq!(payload[0], Tag::GainLegacyChangePatternBank.as_u8());
            assert_eq!(payload[1], segment.as_u8());
        }
    }

    #[test]
    fn stm_kinds_map_onto_their_tags_and_carry_the_transition() {
        let time = DcSysTime::from_nanos(0x0123_4567_89AB_CDEF);
        for (bank, segment) in [
            (PatternBank::B0, Segment::S0),
            (PatternBank::B1, Segment::S1),
        ] {
            for mode in [
                TransitionMode::SyncIdx,
                TransitionMode::SysTime { time, margin: None },
                TransitionMode::Gpio(GpioIn::I2),
                TransitionMode::Ext,
                TransitionMode::Immediate,
            ] {
                for (tag, cmd) in [
                    (
                        Tag::FociStmLegacyChangePatternBank,
                        LegacyChangePatternBank::foci_stm(bank, mode),
                    ),
                    (
                        Tag::GainStmLegacyChangePatternBank,
                        LegacyChangePatternBank::pattern_stm(bank, mode),
                    ),
                ] {
                    let payload = frames(cmd);
                    let expected = transition_mode(mode, 0);
                    assert_eq!(payload[0], tag.as_u8());
                    assert_eq!(payload[1], segment.as_u8());
                    assert_eq!(payload[2], expected.as_u8());
                    assert_eq!(&payload[8..16], &expected.value().to_le_bytes());
                }
            }
        }
    }

    #[test]
    fn the_user_facing_command_packs_exactly_like_the_wire_op() {
        let time = DcSysTime::from_nanos(0x1234);
        let mode = TransitionMode::SysTime { time, margin: None };
        for (cmd, mut op) in [
            (
                LegacyChangePatternBank::pattern(PatternBank::B1),
                op::LegacyChangePatternBank::gain(Segment::S1),
            ),
            (
                LegacyChangePatternBank::foci_stm(PatternBank::B0, mode),
                op::LegacyChangePatternBank::foci_stm(Segment::S0, transition_mode(mode, 0)),
            ),
            (
                LegacyChangePatternBank::pattern_stm(PatternBank::B1, mode),
                op::LegacyChangePatternBank::gain_stm(Segment::S1, transition_mode(mode, 0)),
            ),
        ] {
            let geometry = Geometry::new(vec![Autd3::default()]);
            let mut expected = [0u8; 16];
            let written =
                crate::legacy::op::LegacyOperation::pack(&mut op, &geometry[0], &mut expected[..])
                    .unwrap();
            assert_eq!(&frames(cmd)[..written], &expected[..written]);
        }
    }
}

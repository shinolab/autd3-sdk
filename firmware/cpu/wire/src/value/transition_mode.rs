use super::{GpioIn, SysTime};
use crate::fpga_params::TransitionModeKind;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransitionMode {
    SyncIdx,
    SysTime {
        time: SysTime,
    },
    Gpio(GpioIn),
    Ext,
    #[default]
    Immediate,
}

impl TransitionMode {
    #[must_use]
    pub const fn from_parts(kind: u8, value: u64) -> Option<Self> {
        let Some(kind) = TransitionModeKind::from_u8(kind) else {
            return None;
        };
        match kind {
            TransitionModeKind::SyncIdx => Some(TransitionMode::SyncIdx),
            TransitionModeKind::SysTime => Some(TransitionMode::SysTime {
                time: SysTime::from_nanos(value),
            }),
            TransitionModeKind::Gpio => {
                if value > u8::MAX as u64 {
                    return None;
                }
                match GpioIn::from_u8(value as u8) {
                    Some(pin) => Some(TransitionMode::Gpio(pin)),
                    None => None,
                }
            }
            TransitionModeKind::Ext => Some(TransitionMode::Ext),
            TransitionModeKind::Immediate => Some(TransitionMode::Immediate),
        }
    }

    #[must_use]
    pub const fn kind(self) -> TransitionModeKind {
        match self {
            TransitionMode::SyncIdx => TransitionModeKind::SyncIdx,
            TransitionMode::SysTime { .. } => TransitionModeKind::SysTime,
            TransitionMode::Gpio(_) => TransitionModeKind::Gpio,
            TransitionMode::Ext => TransitionModeKind::Ext,
            TransitionMode::Immediate => TransitionModeKind::Immediate,
        }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            TransitionMode::SysTime { time } => time.sys_time(),
            TransitionMode::Gpio(pin) => pin.as_u8() as u64,
            TransitionMode::SyncIdx | TransitionMode::Ext | TransitionMode::Immediate => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(TransitionMode::SyncIdx, 0x00, 0)]
    #[case(
        TransitionMode::SysTime { time: SysTime::from_nanos(0x0123_4567) },
        0x01,
        0x0123_4567
    )]
    #[case(TransitionMode::Gpio(GpioIn::I3), 0x02, 3)]
    #[case(TransitionMode::Ext, 0xF0, 0)]
    #[case(TransitionMode::Immediate, 0xFF, 0)]
    fn a_transition_mode_round_trips_through_its_kind_byte_and_value(
        #[case] transition: TransitionMode,
        #[case] kind: u8,
        #[case] value: u64,
    ) {
        assert_eq!(transition.kind().as_u8(), kind);
        assert_eq!(transition.value(), value);
        assert_eq!(TransitionMode::from_parts(kind, value), Some(transition));
    }

    #[rstest]
    #[case(4)]
    #[case(0x100)]
    fn a_gpio_value_outside_the_pins_is_rejected(#[case] value: u64) {
        assert_eq!(
            TransitionMode::from_parts(TransitionModeKind::Gpio.as_u8(), value),
            None
        );
    }

    #[test]
    fn an_unknown_kind_byte_is_rejected() {
        assert_eq!(TransitionMode::from_parts(0x03, 0), None);
    }

    #[test]
    fn the_default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

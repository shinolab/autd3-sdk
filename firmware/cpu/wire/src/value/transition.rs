use super::{GpioIn, SysTime};
use crate::fpga_params::TransitionMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transition {
    SyncIdx,
    SysTime(SysTime),
    Gpio(GpioIn),
    Ext,
    Immediate,
}

impl Transition {
    #[must_use]
    pub const fn mode(self) -> TransitionMode {
        match self {
            Transition::SyncIdx => TransitionMode::SyncIdx,
            Transition::SysTime(_) => TransitionMode::SysTime,
            Transition::Gpio(_) => TransitionMode::Gpio,
            Transition::Ext => TransitionMode::Ext,
            Transition::Immediate => TransitionMode::Immediate,
        }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            Transition::SysTime(time) => time.sys_time(),
            Transition::Gpio(pin) => pin.as_u8() as u64,
            Transition::SyncIdx | Transition::Ext | Transition::Immediate => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Transition::SyncIdx, 0x00, 0)]
    #[case(
        Transition::SysTime(SysTime::from_nanos(0x0123_4567)),
        0x01,
        0x0123_4567
    )]
    #[case(Transition::Gpio(GpioIn::I3), 0x02, 3)]
    #[case(Transition::Ext, 0xF0, 0)]
    #[case(Transition::Immediate, 0xFF, 0)]
    fn a_transition_maps_to_its_mode_byte_and_value(
        #[case] transition: Transition,
        #[case] mode: u8,
        #[case] value: u64,
    ) {
        assert_eq!(transition.mode().as_u8(), mode);
        assert_eq!(transition.value(), value);
    }
}

use autd3_rs_core::value::{DcSysTime, GpioIn};

use super::params::{
    TRANSITION_MODE_EXT, TRANSITION_MODE_GPIO, TRANSITION_MODE_IMMEDIATE, TRANSITION_MODE_NONE,
    TRANSITION_MODE_SYNC_IDX, TRANSITION_MODE_SYS_TIME,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Segment {
    #[default]
    S0 = 0,
    S1 = 1,
}

impl Segment {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransitionMode {
    SyncIdx,
    SysTime(DcSysTime),
    Gpio(GpioIn),
    Ext,
    #[default]
    Immediate,
    Later,
}

impl TransitionMode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            TransitionMode::SyncIdx => TRANSITION_MODE_SYNC_IDX,
            TransitionMode::SysTime(_) => TRANSITION_MODE_SYS_TIME,
            TransitionMode::Gpio(_) => TRANSITION_MODE_GPIO,
            TransitionMode::Ext => TRANSITION_MODE_EXT,
            TransitionMode::Immediate => TRANSITION_MODE_IMMEDIATE,
            TransitionMode::Later => TRANSITION_MODE_NONE,
        }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            TransitionMode::SysTime(t) => t.sys_time(),
            TransitionMode::Gpio(g) => g.as_u8() as u64,
            _ => 0,
        }
    }

    #[must_use]
    pub const fn is_later(self) -> bool {
        matches!(self, TransitionMode::Later)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_mode_wire_bytes_match_legacy() {
        assert_eq!(TransitionMode::SyncIdx.as_u8(), 0x00);
        assert_eq!(TransitionMode::SysTime(DcSysTime::ZERO).as_u8(), 0x01);
        assert_eq!(TransitionMode::Gpio(GpioIn::I0).as_u8(), 0x02);
        assert_eq!(TransitionMode::Ext.as_u8(), 0xF0);
        assert_eq!(TransitionMode::Later.as_u8(), 0xFE);
        assert_eq!(TransitionMode::Immediate.as_u8(), 0xFF);
    }

    #[test]
    fn transition_mode_values() {
        assert_eq!(TransitionMode::SyncIdx.value(), 0);
        assert_eq!(
            TransitionMode::SysTime(DcSysTime::from_nanos(0x0123_4567_89AB_CDEF)).value(),
            0x0123_4567_89AB_CDEF
        );
        assert_eq!(TransitionMode::Gpio(GpioIn::I3).value(), 3);
        assert_eq!(TransitionMode::Ext.value(), 0);
        assert_eq!(TransitionMode::Immediate.value(), 0);
        assert_eq!(TransitionMode::Later.value(), 0);
    }

    #[test]
    fn only_later_is_later() {
        assert!(TransitionMode::Later.is_later());
        assert!(!TransitionMode::Immediate.is_later());
    }

    #[test]
    fn segment_wire_bytes() {
        assert_eq!(Segment::S0.as_u8(), 0);
        assert_eq!(Segment::S1.as_u8(), 1);
        assert_eq!(Segment::default(), Segment::S0);
    }
}

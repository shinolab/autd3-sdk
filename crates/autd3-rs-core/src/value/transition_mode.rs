use autd3_cpu_wire::value::Transition;

use super::{GpioIn, SysTime};
use crate::error::EncodeError;

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
    Later,
}

impl TransitionMode {
    #[doc(hidden)]
    pub const fn try_as_wire(self) -> Result<Transition, EncodeError> {
        match self {
            TransitionMode::SyncIdx => Ok(Transition::SyncIdx),
            TransitionMode::SysTime { time } => Ok(Transition::SysTime(time)),
            TransitionMode::Gpio(pin) => Ok(Transition::Gpio(pin)),
            TransitionMode::Ext => Ok(Transition::Ext),
            TransitionMode::Immediate => Ok(Transition::Immediate),
            TransitionMode::Later => Err(EncodeError::TransitionLaterNotEncodable),
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

    fn sys_time(nanos: u64) -> TransitionMode {
        TransitionMode::SysTime {
            time: SysTime::from_nanos(nanos),
        }
    }

    #[test]
    fn wire_mode_bytes() {
        let byte = |mode: TransitionMode| mode.try_as_wire().map(|t| t.mode().as_u8());
        assert_eq!(byte(TransitionMode::SyncIdx), Ok(0x00));
        assert_eq!(byte(sys_time(0)), Ok(0x01));
        assert_eq!(byte(TransitionMode::Gpio(GpioIn::I0)), Ok(0x02));
        assert_eq!(byte(TransitionMode::Ext), Ok(0xF0));
        assert_eq!(byte(TransitionMode::Immediate), Ok(0xFF));
    }

    #[test]
    fn later_has_no_wire_byte() {
        assert_eq!(
            TransitionMode::Later.try_as_wire(),
            Err(EncodeError::TransitionLaterNotEncodable)
        );
    }

    #[test]
    fn only_later_is_later() {
        assert!(TransitionMode::Later.is_later());
        assert!(!TransitionMode::Immediate.is_later());
        assert!(!TransitionMode::Ext.is_later());
    }

    #[test]
    fn wire_values() {
        let value = |mode: TransitionMode| mode.try_as_wire().map(Transition::value);
        assert_eq!(value(TransitionMode::SyncIdx), Ok(0));
        assert_eq!(value(TransitionMode::Immediate), Ok(0));
        assert_eq!(value(TransitionMode::Ext), Ok(0));
        assert_eq!(value(sys_time(0x0123_4567)), Ok(0x0123_4567));
        assert_eq!(value(TransitionMode::Gpio(GpioIn::I3)), Ok(3));
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

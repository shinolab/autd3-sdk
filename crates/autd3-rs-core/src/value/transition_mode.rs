use autd3_cpu_wire::value::Transition;

use super::{GpioIn, SysTime};

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
    #[doc(hidden)]
    #[must_use]
    pub const fn as_wire(self) -> Transition {
        match self {
            TransitionMode::SyncIdx => Transition::SyncIdx,
            TransitionMode::SysTime { time } => Transition::SysTime(time),
            TransitionMode::Gpio(pin) => Transition::Gpio(pin),
            TransitionMode::Ext => Transition::Ext,
            TransitionMode::Immediate => Transition::Immediate,
        }
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
        let byte = |mode: TransitionMode| mode.as_wire().mode().as_u8();
        assert_eq!(byte(TransitionMode::SyncIdx), 0x00);
        assert_eq!(byte(sys_time(0)), 0x01);
        assert_eq!(byte(TransitionMode::Gpio(GpioIn::I0)), 0x02);
        assert_eq!(byte(TransitionMode::Ext), 0xF0);
        assert_eq!(byte(TransitionMode::Immediate), 0xFF);
    }

    #[test]
    fn wire_values() {
        let value = |mode: TransitionMode| mode.as_wire().value();
        assert_eq!(value(TransitionMode::SyncIdx), 0);
        assert_eq!(value(TransitionMode::Immediate), 0);
        assert_eq!(value(TransitionMode::Ext), 0);
        assert_eq!(value(sys_time(0x0123_4567)), 0x0123_4567);
        assert_eq!(value(TransitionMode::Gpio(GpioIn::I3)), 3);
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

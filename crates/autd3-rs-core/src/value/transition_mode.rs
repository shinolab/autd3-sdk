use super::{DcSysTime, GpioIn};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransitionMode {
    SyncIdx,
    SysTime(DcSysTime),
    Gpio(GpioIn),
    Ext,
    #[default]
    Immediate,
}

impl TransitionMode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            TransitionMode::SyncIdx => 0x00,
            TransitionMode::SysTime(_) => 0x01,
            TransitionMode::Gpio(_) => 0x02,
            TransitionMode::Ext => 0xF0,
            TransitionMode::Immediate => 0xFF,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_mode_bytes() {
        assert_eq!(TransitionMode::SyncIdx.as_u8(), 0x00);
        assert_eq!(TransitionMode::SysTime(DcSysTime::ZERO).as_u8(), 0x01);
        assert_eq!(TransitionMode::Gpio(GpioIn::I0).as_u8(), 0x02);
        assert_eq!(TransitionMode::Ext.as_u8(), 0xF0);
        assert_eq!(TransitionMode::Immediate.as_u8(), 0xFF);
    }

    #[test]
    fn wire_values() {
        assert_eq!(TransitionMode::SyncIdx.value(), 0);
        assert_eq!(TransitionMode::Immediate.value(), 0);
        assert_eq!(TransitionMode::Ext.value(), 0);
        assert_eq!(
            TransitionMode::SysTime(DcSysTime::from_nanos(0x0123_4567)).value(),
            0x0123_4567
        );
        assert_eq!(TransitionMode::Gpio(GpioIn::I3).value(), 3);
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

use core::time::Duration;

use super::{DcSysTime, GpioIn};
use crate::error::EncodeError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransitionMode {
    SyncIdx,
    SysTime {
        time: DcSysTime,
        margin: Option<Duration>,
    },
    Gpio(GpioIn),
    Ext,
    #[default]
    Immediate,
    Later,
}

impl TransitionMode {
    #[doc(hidden)]
    pub const fn try_as_u8(self) -> Result<u8, EncodeError> {
        match self {
            TransitionMode::SyncIdx => Ok(autd3_cpu_wire::params::TRANSITION_MODE_SYNC_IDX),
            TransitionMode::SysTime { .. } => Ok(autd3_cpu_wire::params::TRANSITION_MODE_SYS_TIME),
            TransitionMode::Gpio(_) => Ok(autd3_cpu_wire::params::TRANSITION_MODE_GPIO),
            TransitionMode::Ext => Ok(autd3_cpu_wire::params::TRANSITION_MODE_EXT),
            TransitionMode::Immediate => Ok(0xFF),
            TransitionMode::Later => Err(EncodeError::TransitionLaterNotEncodable),
        }
    }

    #[must_use]
    pub const fn is_later(self) -> bool {
        matches!(self, TransitionMode::Later)
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            TransitionMode::SysTime { time, .. } => time.sys_time(),
            TransitionMode::Gpio(g) => g.as_u8() as u64,
            _ => 0,
        }
    }

    #[must_use]
    pub fn with_dc_offset(self, offset_ns: i64) -> Self {
        match self {
            TransitionMode::SysTime { time, margin } => TransitionMode::SysTime {
                time: time.with_dc_offset(offset_ns),
                margin,
            },
            other => other,
        }
    }

    #[doc(hidden)]
    pub fn margin_ns(self) -> Result<u32, EncodeError> {
        let TransitionMode::SysTime {
            margin: Some(margin),
            ..
        } = self
        else {
            return Ok(0);
        };
        u32::try_from(margin.as_nanos())
            .map_err(|_| EncodeError::TransitionMarginOutOfRange(margin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys_time(nanos: u64) -> TransitionMode {
        TransitionMode::SysTime {
            time: DcSysTime::from_nanos(nanos),
            margin: None,
        }
    }

    #[test]
    fn wire_mode_bytes() {
        assert_eq!(TransitionMode::SyncIdx.try_as_u8(), Ok(0x00));
        assert_eq!(sys_time(0).try_as_u8(), Ok(0x01));
        assert_eq!(TransitionMode::Gpio(GpioIn::I0).try_as_u8(), Ok(0x02));
        assert_eq!(TransitionMode::Ext.try_as_u8(), Ok(0xF0));
        assert_eq!(TransitionMode::Immediate.try_as_u8(), Ok(0xFF));
    }

    #[test]
    fn later_has_no_wire_byte() {
        assert_eq!(
            TransitionMode::Later.try_as_u8(),
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
        assert_eq!(TransitionMode::SyncIdx.value(), 0);
        assert_eq!(TransitionMode::Immediate.value(), 0);
        assert_eq!(TransitionMode::Ext.value(), 0);
        assert_eq!(TransitionMode::Later.value(), 0);
        assert_eq!(sys_time(0x0123_4567).value(), 0x0123_4567);
        assert_eq!(TransitionMode::Gpio(GpioIn::I3).value(), 3);
    }

    #[test]
    fn margin_defaults_to_zero_and_encodes_nanos() {
        assert_eq!(TransitionMode::Immediate.margin_ns(), Ok(0));
        assert_eq!(sys_time(0).margin_ns(), Ok(0));
        assert_eq!(
            TransitionMode::SysTime {
                time: DcSysTime::ZERO,
                margin: Some(Duration::from_millis(1)),
            }
            .margin_ns(),
            Ok(1_000_000)
        );
        assert_eq!(
            TransitionMode::SysTime {
                time: DcSysTime::ZERO,
                margin: Some(Duration::from_secs(5)),
            }
            .margin_ns(),
            Err(EncodeError::TransitionMarginOutOfRange(
                Duration::from_secs(5)
            ))
        );
    }

    #[test]
    fn only_sys_time_moves_with_the_bus_clock() {
        assert_eq!(
            sys_time(1_000).with_dc_offset(25),
            sys_time(1_025),
            "SysTime is an absolute instant on the bus clock"
        );
        for mode in [
            TransitionMode::SyncIdx,
            TransitionMode::Gpio(GpioIn::I0),
            TransitionMode::Ext,
            TransitionMode::Immediate,
        ] {
            assert_eq!(mode.with_dc_offset(25), mode);
        }
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

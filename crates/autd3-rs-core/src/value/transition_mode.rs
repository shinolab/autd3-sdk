use core::time::Duration;

use super::{DcSysTime, GpioIn};
use crate::error::PayloadError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
}

impl TransitionMode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            TransitionMode::SyncIdx => autd3_cpu_wire::params::TRANSITION_MODE_SYNC_IDX,
            TransitionMode::SysTime { .. } => autd3_cpu_wire::params::TRANSITION_MODE_SYS_TIME,
            TransitionMode::Gpio(_) => autd3_cpu_wire::params::TRANSITION_MODE_GPIO,
            TransitionMode::Ext => autd3_cpu_wire::params::TRANSITION_MODE_EXT,
            TransitionMode::Immediate => 0xFF,
        }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        match self {
            TransitionMode::SysTime { time, .. } => time.sys_time(),
            TransitionMode::Gpio(g) => g.as_u8() as u64,
            _ => 0,
        }
    }

    pub fn margin_ns(self) -> Result<u32, PayloadError> {
        let TransitionMode::SysTime {
            margin: Some(margin),
            ..
        } = self
        else {
            return Ok(0);
        };
        u32::try_from(margin.as_nanos())
            .map_err(|_| PayloadError::TransitionMarginOutOfRange(margin))
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
        assert_eq!(TransitionMode::SyncIdx.as_u8(), 0x00);
        assert_eq!(sys_time(0).as_u8(), 0x01);
        assert_eq!(TransitionMode::Gpio(GpioIn::I0).as_u8(), 0x02);
        assert_eq!(TransitionMode::Ext.as_u8(), 0xF0);
        assert_eq!(TransitionMode::Immediate.as_u8(), 0xFF);
    }

    #[test]
    fn wire_values() {
        assert_eq!(TransitionMode::SyncIdx.value(), 0);
        assert_eq!(TransitionMode::Immediate.value(), 0);
        assert_eq!(TransitionMode::Ext.value(), 0);
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
            Err(PayloadError::TransitionMarginOutOfRange(
                Duration::from_secs(5)
            ))
        );
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(TransitionMode::default(), TransitionMode::Immediate);
    }
}

use autd3_rs_core::value::DcSysTime;

use super::params::{
    GPIO_O_TYPE_BASE_SIG, GPIO_O_TYPE_DIRECT, GPIO_O_TYPE_FORCE_FAN, GPIO_O_TYPE_IS_STM_MODE,
    GPIO_O_TYPE_MOD_IDX, GPIO_O_TYPE_MOD_SEGMENT, GPIO_O_TYPE_NONE, GPIO_O_TYPE_PWM_OUT,
    GPIO_O_TYPE_STM_IDX, GPIO_O_TYPE_STM_SEGMENT, GPIO_O_TYPE_SYNC, GPIO_O_TYPE_SYNC_DIFF,
    GPIO_O_TYPE_SYS_TIME_EQ, GPIO_O_TYPE_THERMO, GPIO_O_VALUE_MASK,
};

#[must_use]
const fn ec_time_to_gpio_sys_time(ec_time_ns: u64) -> u64 {
    ((ec_time_ns / 3125) << 6) >> 9
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpioOut {
    #[default]
    Off,
    BaseSignal,
    Thermo,
    ForceFan,
    Sync,
    ModSegment,
    ModIdx(u16),
    StmSegment,
    StmIdx(u16),
    IsStmMode,
    SysTimeEq(DcSysTime),
    SyncDiff,
    PwmOut(u8),
    Direct(bool),
}

impl GpioOut {
    #[must_use]
    pub const fn encode(self) -> u64 {
        let (tag, value): (u8, u64) = match self {
            GpioOut::Off => (GPIO_O_TYPE_NONE, 0),
            GpioOut::BaseSignal => (GPIO_O_TYPE_BASE_SIG, 0),
            GpioOut::Thermo => (GPIO_O_TYPE_THERMO, 0),
            GpioOut::ForceFan => (GPIO_O_TYPE_FORCE_FAN, 0),
            GpioOut::Sync => (GPIO_O_TYPE_SYNC, 0),
            GpioOut::ModSegment => (GPIO_O_TYPE_MOD_SEGMENT, 0),
            GpioOut::ModIdx(idx) => (GPIO_O_TYPE_MOD_IDX, idx as u64),
            GpioOut::StmSegment => (GPIO_O_TYPE_STM_SEGMENT, 0),
            GpioOut::StmIdx(idx) => (GPIO_O_TYPE_STM_IDX, idx as u64),
            GpioOut::IsStmMode => (GPIO_O_TYPE_IS_STM_MODE, 0),
            GpioOut::SysTimeEq(t) => (
                GPIO_O_TYPE_SYS_TIME_EQ,
                ec_time_to_gpio_sys_time(t.sys_time()),
            ),
            GpioOut::SyncDiff => (GPIO_O_TYPE_SYNC_DIFF, 0),
            GpioOut::PwmOut(tr) => (GPIO_O_TYPE_PWM_OUT, tr as u64),
            GpioOut::Direct(on) => (GPIO_O_TYPE_DIRECT, on as u64),
        };
        ((tag as u64) << 56) | (value & GPIO_O_VALUE_MASK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_tags_match_the_legacy_sdk() {
        let tag = |g: GpioOut| (g.encode() >> 56) as u8;
        assert_eq!(tag(GpioOut::Off), 0x00);
        assert_eq!(tag(GpioOut::BaseSignal), 0x01);
        assert_eq!(tag(GpioOut::Thermo), 0x02);
        assert_eq!(tag(GpioOut::ForceFan), 0x03);
        assert_eq!(tag(GpioOut::Sync), 0x10);
        assert_eq!(tag(GpioOut::ModSegment), 0x20);
        assert_eq!(tag(GpioOut::ModIdx(0)), 0x21);
        assert_eq!(tag(GpioOut::StmSegment), 0x50);
        assert_eq!(tag(GpioOut::StmIdx(0)), 0x51);
        assert_eq!(tag(GpioOut::IsStmMode), 0x52);
        assert_eq!(tag(GpioOut::SysTimeEq(DcSysTime::ZERO)), 0x60);
        assert_eq!(tag(GpioOut::SyncDiff), 0x70);
        assert_eq!(tag(GpioOut::PwmOut(0)), 0xE0);
        assert_eq!(tag(GpioOut::Direct(false)), 0xF0);
    }

    #[test]
    fn values_live_in_the_low_56_bits() {
        assert_eq!(GpioOut::ModIdx(0x1234).encode() & GPIO_O_VALUE_MASK, 0x1234);
        assert_eq!(GpioOut::StmIdx(0xABCD).encode() & GPIO_O_VALUE_MASK, 0xABCD);
        assert_eq!(GpioOut::PwmOut(248).encode() & GPIO_O_VALUE_MASK, 248);
        assert_eq!(GpioOut::Direct(true).encode() & GPIO_O_VALUE_MASK, 1);
        assert_eq!(GpioOut::Direct(false).encode() & GPIO_O_VALUE_MASK, 0);
    }

    #[test]
    fn sys_time_eq_is_scaled_the_same_way_as_the_legacy_sdk() {
        let ns = 3125u64 * 4096;
        assert_eq!(
            GpioOut::SysTimeEq(DcSysTime::from_nanos(ns)).encode() & GPIO_O_VALUE_MASK,
            ((ns / 3125) << 6) >> 9
        );
    }
}

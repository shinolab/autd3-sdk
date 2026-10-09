use super::SysTime;
use crate::fpga_params::GpioOutType;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GpioIn {
    #[default]
    I0 = 0,
    I1 = 1,
    I2 = 2,
    I3 = 3,
}

impl GpioIn {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

const GPIO_OUT_VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

const fn gpio_sys_time(sys_time_ns: u64) -> u64 {
    ((sys_time_ns / 3125) << 6) >> 9
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum GpioOut {
    #[default]
    Off,
    BaseSignal,
    Thermo,
    ForceFan,
    Sync,
    ModBank,
    ModIdx(u16),
    PatternBank,
    PatternIdx(u16),
    IsStmMode,
    SysTimeEq(SysTime),
    SyncDiff,
    PwmOut(u8),
    Direct(bool),
}

impl GpioOut {
    #[must_use]
    pub const fn encode(self) -> u64 {
        let (tag, value) = match self {
            GpioOut::Off => (GpioOutType::None, 0),
            GpioOut::BaseSignal => (GpioOutType::BaseSig, 0),
            GpioOut::Thermo => (GpioOutType::Thermo, 0),
            GpioOut::ForceFan => (GpioOutType::ForceFan, 0),
            GpioOut::Sync => (GpioOutType::Sync, 0),
            GpioOut::ModBank => (GpioOutType::ModBank, 0),
            GpioOut::ModIdx(idx) => (GpioOutType::ModIdx, idx as u64),
            GpioOut::PatternBank => (GpioOutType::PatternBank, 0),
            GpioOut::PatternIdx(idx) => (GpioOutType::PatternIdx, idx as u64),
            GpioOut::IsStmMode => (GpioOutType::IsStmMode, 0),
            GpioOut::SysTimeEq(t) => (GpioOutType::SysTimeEq, gpio_sys_time(t.sys_time())),
            GpioOut::SyncDiff => (GpioOutType::SyncDiff, 0),
            GpioOut::PwmOut(tr) => (GpioOutType::PwmOut, tr as u64),
            GpioOut::Direct(on) => (GpioOutType::Direct, on as u64),
        };
        ((tag.as_u8() as u64) << 56) | (value & GPIO_OUT_VALUE_MASK)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const fn tagged(tag: GpioOutType, value: u64) -> u64 {
        ((tag.as_u8() as u64) << 56) | value
    }

    #[rstest]
    #[case(GpioOut::Off, 0)]
    #[case(GpioOut::BaseSignal, tagged(GpioOutType::BaseSig, 0))]
    #[case(GpioOut::ModIdx(0x1234), tagged(GpioOutType::ModIdx, 0x1234))]
    #[case(GpioOut::PatternIdx(0xFFFF), tagged(GpioOutType::PatternIdx, 0xFFFF))]
    #[case(GpioOut::PwmOut(7), tagged(GpioOutType::PwmOut, 7))]
    #[case(GpioOut::Direct(true), tagged(GpioOutType::Direct, 1))]
    #[case(GpioOut::Direct(false), tagged(GpioOutType::Direct, 0))]
    fn an_output_carries_its_tag_in_the_top_byte(#[case] output: GpioOut, #[case] encoded: u64) {
        assert_eq!(output.encode(), encoded);
    }

    #[rstest]
    #[case(0, 0)]
    #[case(24_999, 0)]
    #[case(25_000, 1)]
    #[case(1_000_000_000, 40_000)]
    fn a_sys_time_is_compared_in_ultrasound_periods(#[case] ns: u64, #[case] periods: u64) {
        assert_eq!(
            GpioOut::SysTimeEq(SysTime::from_nanos(ns)).encode(),
            tagged(GpioOutType::SysTimeEq, periods)
        );
    }

    #[test]
    fn a_sys_time_never_spills_into_the_tag() {
        assert_eq!(
            GpioOut::SysTimeEq(SysTime::from_nanos(u64::MAX)).encode() >> 56,
            u64::from(GpioOutType::SysTimeEq.as_u8())
        );
    }
}

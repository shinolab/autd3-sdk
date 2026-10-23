use core::num::NonZeroU16;
use core::time::Duration;

use crate::cpu_params;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct MaxFreqPpb(u32);

impl MaxFreqPpb {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(cpu_params::PTP_MAX_FREQ_PPB_MAX);

    #[must_use]
    pub const fn new(ppb: u32) -> Option<Self> {
        if ppb > cpu_params::PTP_MAX_FREQ_PPB_MAX {
            return None;
        }
        Some(Self(ppb))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for MaxFreqPpb {
    type Error = u32;

    fn try_from(ppb: u32) -> Result<Self, u32> {
        Self::new(ppb).ok_or(ppb)
    }
}

impl From<MaxFreqPpb> for u32 {
    fn from(ppb: MaxFreqPpb) -> Self {
        ppb.get()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PathDelayFilterShift(u8);

impl PathDelayFilterShift {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(cpu_params::PTP_PATH_DELAY_FILTER_SHIFT_MAX);

    #[must_use]
    pub const fn new(shift: u8) -> Option<Self> {
        if shift > cpu_params::PTP_PATH_DELAY_FILTER_SHIFT_MAX {
            return None;
        }
        Some(Self(shift))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for PathDelayFilterShift {
    type Error = u8;

    fn try_from(shift: u8) -> Result<Self, u8> {
        Self::new(shift).ok_or(shift)
    }
}

impl From<PathDelayFilterShift> for u8 {
    fn from(shift: PathDelayFilterShift) -> Self {
        shift.get()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PtpConfig {
    pub sync_interval: Duration,
    pub tx_timestamp_timeout: Duration,
    pub delay_resp_timeout: Duration,
    pub holdover: Duration,
    pub lock_samples: NonZeroU16,
    pub step_threshold: Duration,
    pub lock_threshold: Duration,
    pub kp_milli: u32,
    pub ki_milli: u32,
    pub max_freq_ppb: MaxFreqPpb,
    pub delay_req_syncs: NonZeroU16,
    pub path_delay_filter_shift: PathDelayFilterShift,
    pub pause_quanta: Option<NonZeroU16>,
    pub pause_hold_syncs: u16,
    pub pause_retry: Duration,
}

impl PtpConfig {
    pub const DEFAULT: Self = Self {
        sync_interval: cpu_params::PTP_SYNC_INTERVAL,
        tx_timestamp_timeout: cpu_params::PTP_TX_TIMESTAMP_TIMEOUT,
        delay_resp_timeout: cpu_params::PTP_DELAY_RESP_TIMEOUT,
        holdover: cpu_params::PTP_HOLDOVER,
        lock_samples: cpu_params::PTP_LOCK_SAMPLES,
        step_threshold: cpu_params::PTP_STEP_THRESHOLD,
        lock_threshold: cpu_params::PTP_LOCK_THRESHOLD,
        kp_milli: cpu_params::PTP_KP_MILLI,
        ki_milli: cpu_params::PTP_KI_MILLI,
        max_freq_ppb: cpu_params::PTP_MAX_FREQ_PPB,
        delay_req_syncs: cpu_params::PTP_DELAY_REQ_SYNCS,
        path_delay_filter_shift: cpu_params::PTP_PATH_DELAY_FILTER_SHIFT,
        pause_quanta: Some(cpu_params::PTP_PAUSE_QUANTA),
        pause_hold_syncs: cpu_params::PTP_PAUSE_HOLD_SYNCS,
        pause_retry: cpu_params::PTP_PAUSE_RETRY,
    };
}

impl Default for PtpConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

crate::wire_enum_u8! {
    #[derive(Default)]
    pub enum FpgaBusWait {
        Cycles2 = 2,
        #[default]
        Cycles3 = 3,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CpuConfig {
    pub sync_guard: Duration,
    pub failsafe_timeout: Option<Duration>,
    pub ptp_unlock_failsafe_timeout: Option<Duration>,
    pub fpga_bus_wait: FpgaBusWait,
    pub cpu_watchdog_timeout: Option<Duration>,
    pub ptp: PtpConfig,
}

impl CpuConfig {
    pub const DEFAULT: Self = Self {
        sync_guard: cpu_params::SYNC_GUARD,
        failsafe_timeout: None,
        ptp_unlock_failsafe_timeout: None,
        fpga_bus_wait: FpgaBusWait::Cycles3,
        cpu_watchdog_timeout: Some(cpu_params::CPU_WATCHDOG_TIMEOUT),
        ptp: PtpConfig::DEFAULT,
    };
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FpgaConfig {
    pub sync_phase_lock: bool,
}

impl FpgaConfig {
    pub const DEFAULT: Self = Self {
        sync_phase_lock: cpu_params::SYNC_PHASE_LOCK,
    };
}

impl Default for FpgaConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, Some(0))]
    #[case(500_000, Some(500_000))]
    #[case(0x7FFF_FFFF, Some(0x7FFF_FFFF))]
    #[case(0x8000_0000, None)]
    #[case(u32::MAX, None)]
    fn a_max_freq_ppb_holds_only_what_an_i32_can(#[case] raw: u32, #[case] expected: Option<u32>) {
        assert_eq!(MaxFreqPpb::new(raw).map(MaxFreqPpb::get), expected);
        assert_eq!(
            MaxFreqPpb::try_from(raw).map(u32::from),
            expected.ok_or(raw)
        );
    }

    #[rstest]
    #[case(0, Some(0))]
    #[case(5, Some(5))]
    #[case(16, Some(16))]
    #[case(17, None)]
    #[case(u8::MAX, None)]
    fn a_path_delay_filter_shift_holds_only_zero_to_sixteen(
        #[case] raw: u8,
        #[case] expected: Option<u8>,
    ) {
        assert_eq!(
            PathDelayFilterShift::new(raw).map(PathDelayFilterShift::get),
            expected
        );
        assert_eq!(
            PathDelayFilterShift::try_from(raw).map(u8::from),
            expected.ok_or(raw)
        );
    }

    #[test]
    fn the_bounds_are_the_ends_of_the_range() {
        assert_eq!(MaxFreqPpb::MIN.get(), 0);
        assert_eq!(MaxFreqPpb::MAX.get(), i32::MAX.unsigned_abs());
        assert_eq!(PathDelayFilterShift::MIN.get(), 0);
        assert_eq!(PathDelayFilterShift::MAX.get(), 16);
    }
}

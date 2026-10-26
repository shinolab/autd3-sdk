use core::num::NonZeroU16;
use core::time::Duration;

use crate::cpu_params;

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
    pub max_freq_ppb: u32,
    pub delay_req_syncs: NonZeroU16,
    pub path_delay_filter_shift: u8,
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

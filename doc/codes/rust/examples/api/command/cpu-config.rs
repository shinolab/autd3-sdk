use std::num::{NonZeroU16, NonZeroU32};
use std::time::Duration;

use autd3_rs::commands::{CpuConfig, FpgaBusWait, PtpConfig, SetCpuConfig};

fn main() {
    // ANCHOR: api
    let config = CpuConfig {
        sys_time_transition_margin: Duration::from_millis(10),
        fpga_wait_update_max_polls: NonZeroU32::new(1_000_000).unwrap(),
        fpga_flash_max_polls: NonZeroU32::new(2_000_000_000).unwrap(),
        sync_guard: Duration::from_micros(250),
        update_activate_delay: Duration::from_millis(100),
        failsafe_timeout: Some(Duration::from_millis(500)),
        ptp_unlock_failsafe_timeout: None,
        fpga_bus_wait: FpgaBusWait::Cycles3,
        ptp: PtpConfig {
            sync_interval: Duration::from_millis(8),
            tx_timestamp_timeout: Duration::from_millis(3),
            delay_resp_timeout: Duration::from_millis(6),
            holdover: Duration::from_secs(1),
            lock_samples: NonZeroU16::new(64).unwrap(),
            step_threshold: Duration::from_micros(10),
            lock_threshold: Duration::from_nanos(100),
            kp_milli: 50,
            ki_milli: 1,
            max_freq_ppb: 500_000,
            delay_req_syncs: NonZeroU16::new(32).unwrap(),
            path_delay_filter_shift: 5,
            pause_quanta: NonZeroU16::new(48),
            pause_hold_syncs: 64,
            pause_retry: Duration::from_millis(8),
        },
    };
    SetCpuConfig::new(config);
    // ANCHOR_END: api
}

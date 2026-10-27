use std::num::NonZeroU16;
use std::time::Duration;

use autd3_rs::commands::{
    CpuConfig, FpgaBusWait, PtpConfig, SetCpuConfig,
};

fn main() {
    let sync_guard = Duration::from_micros(250);
    let failsafe_timeout = Some(Duration::from_millis(500));
    let ptp_unlock_failsafe_timeout = None;
    let fpga_bus_wait = FpgaBusWait::Cycles3;
    let cpu_watchdog_timeout = Some(Duration::from_secs(1));
    let sync_interval = Duration::from_millis(8);
    let tx_timestamp_timeout = Duration::from_millis(3);
    let delay_resp_timeout = Duration::from_millis(6);
    let holdover = Duration::from_secs(1);
    let lock_samples = NonZeroU16::new(64).unwrap();
    let step_threshold = Duration::from_micros(10);
    let lock_threshold = Duration::from_nanos(100);
    let kp_milli = 50;
    let ki_milli = 1;
    let max_freq_ppb = 500_000;
    let delay_req_syncs = NonZeroU16::new(32).unwrap();
    let path_delay_filter_shift = 5;
    let pause_quanta = NonZeroU16::new(48);
    let pause_hold_syncs = 64;
    let pause_retry = Duration::from_millis(8);
    // ANCHOR: api
    let config = CpuConfig {
        sync_guard,
        failsafe_timeout,
        ptp_unlock_failsafe_timeout,
        fpga_bus_wait,
        cpu_watchdog_timeout,
        ptp: PtpConfig {
            sync_interval,
            tx_timestamp_timeout,
            delay_resp_timeout,
            holdover,
            lock_samples,
            step_threshold,
            lock_threshold,
            kp_milli,
            ki_milli,
            max_freq_ppb,
            delay_req_syncs,
            path_delay_filter_shift,
            pause_quanta,
            pause_hold_syncs,
            pause_retry,
        },
    };
    SetCpuConfig::new(config);
    // ANCHOR_END: api
}

from autd3 import Duration
from autd3.commands import CpuConfig, FpgaBusWait, PtpConfig, SetCpuConfig

# ANCHOR: api
config = CpuConfig(
    sys_time_transition_margin=Duration.from_millis(10),
    fpga_wait_update_max_polls=1_000_000,
    fpga_flash_max_polls=2_000_000_000,
    sync_guard=Duration.from_micros(250),
    update_activate_delay=Duration.from_millis(100),
    failsafe_timeout=Duration.from_millis(500),
    ptp_unlock_failsafe_timeout=None,
    fpga_bus_wait=FpgaBusWait.Cycles3,
    ptp=PtpConfig(
        sync_interval=Duration.from_millis(8),
        tx_timestamp_timeout=Duration.from_millis(3),
        delay_resp_timeout=Duration.from_millis(6),
        holdover=Duration.from_secs(1),
        lock_samples=64,
        step_threshold=Duration.from_micros(10),
        lock_threshold=Duration.from_nanos(100),
        kp_milli=50,
        ki_milli=1,
        max_freq_ppb=500_000,
        delay_req_syncs=32,
        path_delay_filter_shift=5,
        pause_quanta=48,
        pause_hold_syncs=64,
        pause_retry=Duration.from_millis(8),
    ),
)
SetCpuConfig(config)
# ANCHOR_END: api

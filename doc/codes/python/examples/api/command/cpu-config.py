from autd3 import Duration
from autd3.commands import CpuConfig, FpgaBusWait, PtpConfig, SetCpuConfig

sync_guard = Duration.from_micros(250)
failsafe_timeout = Duration.from_millis(500)
ptp_unlock_failsafe_timeout = None
fpga_bus_wait = FpgaBusWait.Cycles3
cpu_watchdog_timeout = Duration.from_secs(1)
sync_interval = Duration.from_millis(8)
tx_timestamp_timeout = Duration.from_millis(3)
delay_resp_timeout = Duration.from_millis(6)
holdover = Duration.from_secs(1)
lock_samples = 64
step_threshold = Duration.from_micros(10)
lock_threshold = Duration.from_nanos(100)
kp_milli = 50
ki_milli = 1
max_freq_ppb = 500_000
delay_req_syncs = 32
path_delay_filter_shift = 5
pause_quanta = 48
pause_hold_syncs = 64
pause_retry = Duration.from_millis(8)
# ANCHOR: api
config = CpuConfig(
    sync_guard=sync_guard,
    failsafe_timeout=failsafe_timeout,
    ptp_unlock_failsafe_timeout=ptp_unlock_failsafe_timeout,
    fpga_bus_wait=fpga_bus_wait,
    cpu_watchdog_timeout=cpu_watchdog_timeout,
    ptp=PtpConfig(
        sync_interval=sync_interval,
        tx_timestamp_timeout=tx_timestamp_timeout,
        delay_resp_timeout=delay_resp_timeout,
        holdover=holdover,
        lock_samples=lock_samples,
        step_threshold=step_threshold,
        lock_threshold=lock_threshold,
        kp_milli=kp_milli,
        ki_milli=ki_milli,
        max_freq_ppb=max_freq_ppb,
        delay_req_syncs=delay_req_syncs,
        path_delay_filter_shift=path_delay_filter_shift,
        pause_quanta=pause_quanta,
        pause_hold_syncs=pause_hold_syncs,
        pause_retry=pause_retry,
    ),
)
SetCpuConfig(config)
# ANCHOR_END: api

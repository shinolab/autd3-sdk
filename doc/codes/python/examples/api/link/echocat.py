from autd3 import Duration

import autd3_link_echocat as echocat

iface = "eth0"
sync0_period = Duration.from_millis(1)
pdu_timeout = Duration.from_millis(100)
state_transition_timeout = Duration.from_secs(10)
dc_static_sync_iterations = 10000
dc_start_delay = Duration.from_millis(100)
dc_sync_tolerance = Duration.from_micros(1)
dc_sync_timeout = Duration.from_secs(10)
process_data_watchdog = Duration.from_millis(100)
spin_margin = None
# ANCHOR: api
echocat.EchocatLinkOption(
    iface,
    sync0_period,
    pdu_timeout,
    state_transition_timeout,
    dc_static_sync_iterations,
    dc_start_delay,
    dc_sync_tolerance,
    dc_sync_timeout,
    process_data_watchdog,
    spin_margin,
)
# ANCHOR_END: api

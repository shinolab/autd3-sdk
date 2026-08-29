from autd3 import Duration

from autd3_link_echocat import FramePhase, EchocatLinkOption

iface = "eth0"
sync0_period = Duration.from_millis(1)
frame_phase = FramePhase.Auto
pdu_timeout = Duration.from_millis(100)
state_transition_timeout = Duration.from_secs(10)
dc_static_sync_iterations = 10000
dc_start_delay = Duration.from_millis(100)
sync_tolerance = Duration.from_micros(1)
sync_timeout = Duration.from_secs(10)
process_data_watchdog = Duration.from_millis(100)
spin_margin = None
# ANCHOR: api
EchocatLinkOption(
    iface,
    sync0_period,
    frame_phase,
    pdu_timeout,
    state_transition_timeout,
    dc_static_sync_iterations,
    dc_start_delay,
    sync_tolerance,
    sync_timeout,
    process_data_watchdog,
    spin_margin,
)
# ANCHOR_END: api

# ANCHOR: frame_phase
FramePhase.Auto
FramePhase.At(Duration.from_micros(500))
# ANCHOR_END: frame_phase

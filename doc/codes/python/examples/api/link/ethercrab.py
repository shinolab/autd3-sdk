from autd3 import Duration

import autd3_link_ethercrab as ethercrab

interface = "eth0"
sync0_period = Duration.from_millis(1)
sync0_shift = Duration.from_millis(0)
sync_tolerance = Duration.from_micros(1)
sync_timeout = Duration.from_secs(10)
# ANCHOR: api
ethercrab.EtherCrabLinkOption(
    interface,
    sync0_period,
    sync0_shift,
    sync_tolerance,
    sync_timeout,
)
# ANCHOR_END: api

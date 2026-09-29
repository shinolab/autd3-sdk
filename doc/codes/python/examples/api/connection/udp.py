from autd3 import Duration, TransportOption


iface = "eth0"
heartbeat = Duration.from_millis(10)
reply_timeout = Duration.from_millis(1)
lost_timeout = Duration.from_millis(100)
response_timeout = Duration.from_millis(200)
enumeration_timeout = Duration.from_secs(10)
sync_timeout = Duration.from_secs(5)
# ANCHOR: api
TransportOption(
    iface=iface,
    heartbeat=heartbeat,
    reply_timeout=reply_timeout,
    lost_timeout=lost_timeout,
    response_timeout=response_timeout,
    enumeration_timeout=enumeration_timeout,
    sync_timeout=sync_timeout,
)
# ANCHOR_END: api

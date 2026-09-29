from autd3 import Duration, TransportOption


iface = "eth0"
cycle = Duration.from_millis(1)
reply_timeout = Duration.from_millis(1)
response_timeout = Duration.from_millis(200)
enumeration_timeout = Duration.from_secs(10)
sync_timeout = Duration.from_secs(5)
# ANCHOR: api
TransportOption(
    iface=iface,
    cycle=cycle,
    reply_timeout=reply_timeout,
    response_timeout=response_timeout,
    enumeration_timeout=enumeration_timeout,
    sync_timeout=sync_timeout,
)
# ANCHOR_END: api

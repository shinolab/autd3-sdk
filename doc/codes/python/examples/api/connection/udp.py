from autd3 import Duration, Interface, TransportOption


iface = Interface.Auto
heartbeat = Duration.from_millis(10)
reply_timeout = Duration.from_millis(1)
lost_timeout = Duration.from_millis(100)
response_timeout = Duration.from_millis(200)
enumeration_timeout = Duration.from_secs(10)
sync_timeout = Duration.from_secs(30)
send_rate_limit = None
send_buffer = 8192
timer_resolution = Duration.from_millis(1)
# ANCHOR: api
TransportOption(
    iface=iface,
    heartbeat=heartbeat,
    reply_timeout=reply_timeout,
    lost_timeout=lost_timeout,
    response_timeout=response_timeout,
    enumeration_timeout=enumeration_timeout,
    sync_timeout=sync_timeout,
    send_rate_limit=send_rate_limit,
    send_buffer=send_buffer,
    timer_resolution=timer_resolution,
)
# ANCHOR_END: api

# ANCHOR: iface
Interface.Auto
Interface.Name("eth0")
Interface.Simulator
# ANCHOR_END: iface

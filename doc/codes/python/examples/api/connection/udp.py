from autd3 import Duration, Interface, TransportOption
from autd3.params import SIMULATOR_PORT


iface = Interface.Auto
reply_timeout = Duration.from_millis(1)
lost_timeout = Duration.from_millis(100)
response_timeout = Duration.from_millis(200)
enumeration_timeout = Duration.from_secs(10)
sync_timeout = Duration.from_secs(30)
ack_timeout = Duration.from_millis(10)
send_rate_limit = None
send_buffer = 8192
# ANCHOR: api
TransportOption(
    iface=iface,
    reply_timeout=reply_timeout,
    lost_timeout=lost_timeout,
    response_timeout=response_timeout,
    enumeration_timeout=enumeration_timeout,
    sync_timeout=sync_timeout,
    ack_timeout=ack_timeout,
    send_rate_limit=send_rate_limit,
    send_buffer=send_buffer,
)
# ANCHOR_END: api

# ANCHOR: iface
Interface.Auto
Interface.Name("eth0")
Interface.Simulator(SIMULATOR_PORT)
Interface.Addr("[::1]:44336")
# ANCHOR_END: iface

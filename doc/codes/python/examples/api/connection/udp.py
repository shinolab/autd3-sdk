from autd3 import Duration, Interface, TransportOption


iface = Interface.Auto
open_timeout = Duration.from_secs(40)
recovery_reply_timeout = Duration.from_millis(1)
lost_timeout = Duration.from_millis(100)
ack_timeout = Duration.from_millis(10)
send_buffer_bytes = 8192
# ANCHOR: api
TransportOption(
    iface=iface,
    open_timeout=open_timeout,
    recovery_reply_timeout=recovery_reply_timeout,
    lost_timeout=lost_timeout,
    ack_timeout=ack_timeout,
    send_buffer_bytes=send_buffer_bytes,
)
# ANCHOR_END: api

# ANCHOR: iface
Interface.Auto
Interface.Name("eth0")
Interface.Addr("[::1]:44336")
# ANCHOR_END: iface

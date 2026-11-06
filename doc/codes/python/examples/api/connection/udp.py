from autd3 import Duration, Interface, TransportOption


iface = Interface.Auto
open_timeout = Duration.from_secs(40)
send_buffer_bytes = 8192
sync_on_open = True
heartbeat_interval = None
# ANCHOR: api
TransportOption(
    iface=iface,
    open_timeout=open_timeout,
    send_buffer_bytes=send_buffer_bytes,
    sync_on_open=sync_on_open,
    heartbeat_interval=heartbeat_interval,
)
# ANCHOR_END: api

# ANCHOR: iface
Interface.Auto
Interface.Name("eth0")
Interface.Addr("[::1]:44336")
# ANCHOR_END: iface

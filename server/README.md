# autd3-server

Standing server for AUTD3 devices.

It enumerates and synchronizes the devices once and keeps driving them over UDP: heartbeats, lost-device
detection, automatic recovery and the send rate limit all live in the server. A client on the same host
reaches the devices through it over TCP on the loopback address, so reopening a client skips the
enumeration and the PTP lock wait of the devices, and the state of the devices can be watched while no
client is running.

```console
autd3-server --devices 4 --iface eth0 --recovery-interval 100ms
```

A client connects with `ServerLink` in place of `UdpOption`:

```rust
let client = Client::open(&geometry, ServerLink::default(), ClientConfig::default()).await?;
```

- The UDP options (`--heartbeat`, `--no-heartbeat`, `--reply-timeout`, `--lost-timeout`,
  `--recovery-interval`, `--response-timeout`, `--enumeration-timeout`, `--sync-timeout`,
  `--send-rate-limit`, `--send-buffer`, `--timer-resolution`, `--no-timer-resolution`) are the server's.
  The client keeps its `ClientConfig`.
- The server sends heartbeats and recovery probes only while a client is attached and never sends a frame
  of its own. Once the client disconnects (also when its process dies) the devices stop their output
  through their failsafe.
- One client at a time. A second client, a client whose geometry has another device count, and any client
  while the server is still enumerating are refused.
- Logs go to stderr. One JSON line describing the devices goes to stdout every `--monitor-interval`.
- When a device stops answering or is lost, the server enumerates again once no client is attached.
- `Interface::Auto` of a direct client does not look for the server. Do not open a direct client while the
  server runs: it enumerates the devices again and breaks the server's session.
- The server relays real devices only; it does not serve the simulator.

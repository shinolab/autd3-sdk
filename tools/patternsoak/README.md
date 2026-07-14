# autd3-rs-patternsoak

A CLI tool that sends `WritePatternBuffer` continuously over a real EtherCAT link for an extended period,
to confirm the CPU board stays healthy under a sustained stream of pattern writes.

On start-up it also issues `SetGpioOut` so that GPIO[0] of every device outputs `BaseSignal`. Probing GPIO[0]
across devices with an oscilloscope shows whether they are still synchronized during the soak.

## Run

The `ethercrab` and `soem` links use raw sockets, so they need privileges: root or `CAP_NET_RAW` on Linux,
read/write access to `/dev/bpf*` on macOS. The easiest way is through xtask; it builds in release mode and
then runs the binary under `sudo`. The `twincat` link talks to the TwinCAT runtime over ADS on Windows and
needs no such privileges.

```sh
# Run until Ctrl+C (stop-and-wait)
cargo xtask tool patternsoak -- --interface enp3s0

# Pipelined streaming run for a fixed duration — pushes the frame rate
cargo xtask tool patternsoak -- --interface enp3s0 --duration 1h --mode streaming

# Run on the SOEM link for a fixed number of sends
cargo xtask tool patternsoak -- --link soem --interface enp3s0 --count 1000000

# Run against a local TwinCAT runtime (Windows; the TwinCAT driver owns the NIC, so no --interface)
cargo xtask tool patternsoak -- --link twincat --duration 1h

# Run against a remote TwinCAT host over ADS
cargo xtask tool patternsoak -- --link twincat --twincat-remote 192.168.0.1 --ams-net-id 192.168.0.1.1.1
```

## Arguments

| Flag                      | Description |
|---------------------------|-------------|
| `--link <KIND>`           | `ethercrab` (default), `soem`, or `twincat`. |
| `--interface <NAME>`      | EtherCAT network interface. Not valid with `--link twincat`. |
| `--devices <N>`           | Fail unless exactly N devices are on the bus. |
| `--twincat-remote <IP>`   | Connect to a remote TwinCAT host over ADS (requires `--ams-net-id`). Omit for a local TwinCAT runtime. `--link twincat` only. |
| `--ams-net-id <ID>`       | AMS Net ID of the remote target, e.g. `192.168.0.1.1.1`. `--link twincat` only. |
| `--count <N>`             | Stop after N sends. Default: run until Ctrl+C. |
| `--duration <DUR>`        | Stop after this wall-clock duration. Default: run until Ctrl+C. |
| `--mode <MODE>`           | `stop-and-wait` (default) or `streaming`. |
| `--inflight <N>`          | Pipeline depth in `streaming` mode. Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. |
| `--stop-on-error`         | Abort on the first send error instead of counting and continuing. Default: off. |
| `--low-latency`           | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path. Default: off. Lets the same soak be run against both modes. |
| `--cycle-us <N>`          | EtherCAT cycle period in microseconds. Default = 1000. Ignored with `--link twincat` (the cycle time comes from the TwinCAT task configuration). |
| `--timeout-cycles <N>`    | PDO cycles to wait for an ACK before raising `Timeout`. Default = 10. |
| `--send-interval-cycles <N>` | Minimum PDO cycles between consecutive command pickups. Default = 1. |
| `--max-resync-rounds <N>` | Go-back-N resync give-up bound. Default = 8. |

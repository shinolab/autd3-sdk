# autd3-rs-patternsoak

A CLI tool that sends `WritePatternBuffer` continuously over a real EtherCAT link for an extended period,
to confirm the CPU board stays healthy under a sustained stream of pattern writes.

## Run

The link uses raw sockets, so on Linux it must be run as root or with `CAP_NET_RAW`.
The easiest way is through xtask; it builds in release mode and then runs the binary under `sudo`:

```sh
# Run until Ctrl+C (stop-and-wait)
cargo xtask tool patternsoak -- --interface enp3s0

# Pipelined streaming run for a fixed duration — pushes the frame rate
cargo xtask tool patternsoak -- --interface enp3s0 --duration 1h --mode streaming

# Run on the SOEM link for a fixed number of sends
cargo xtask tool patternsoak -- --link soem --interface enp3s0 --count 1000000
```

## Arguments

| Flag                      | Description |
|---------------------------|-------------|
| `--link <KIND>`           | `ethercrab` (default) or `soem`. |
| `--interface <NAME>`      | EtherCAT network interface. |
| `--devices <N>`           | Fail unless exactly N devices are on the bus. |
| `--count <N>`             | Stop after N sends. Default: run until Ctrl+C. |
| `--duration <DUR>`        | Stop after this wall-clock duration. Default: run until Ctrl+C. |
| `--mode <MODE>`           | `stop-and-wait` (default) or `streaming`. |
| `--inflight <N>`          | Pipeline depth in `streaming` mode. Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. |
| `--stop-on-error`         | Abort on the first send error instead of counting and continuing. Default: off. |
| `--low-latency`           | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path. Default: off. Lets the same soak be run against both modes. |
| `--cycle-us <N>`          | EtherCAT cycle period in microseconds. Default = 1000. |
| `--timeout-cycles <N>`    | PDO cycles to wait for an ACK before raising `Timeout`. Default = 10. |
| `--send-interval-cycles <N>` | Minimum PDO cycles between consecutive command pickups. Default = 1. |
| `--max-resync-rounds <N>` | Go-back-N resync give-up bound. Default = 8. |

# autd3-rs-perftest

A CLI tool that streams `XorHash` commands over a real EtherCAT link and reports latency/throughput statistics.
Useful for sanity-checking the firmware ↔ host protocol, comparing kernels/NICs, and watching for regressions in `autd3-rs`'s request-response engine.

## Run

The link uses raw sockets, so on Linux it must be run as root or with `CAP_NET_RAW`.
The easiest way is through xtask; it builds in release mode and then runs the binary under `sudo`:

```sh
# 10,000 commands as fast as the bus allows by stop-and-wait manner
cargo xtask tool perftest -- --interface enp3s0 --count 10000

# Pipelined streaming run — measures the 1-frame-per-cycle ceiling
cargo xtask tool perftest -- --interface enp3s0 --count 10000 --mode streaming
```

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--link <KIND>`       | `ethercrab` (default), `soem`, or `twincat`. |
| `--interface <NAME>`  | EtherCAT network interface (for `ethercrab` / `soem`). |
| `--devices <N>`       | Expected device count. Required for `twincat` (no bus scan); a mismatch guard otherwise. |
| `--twincat-remote <IP>` | Connect to a remote TwinCAT host over ADS (requires `--ams-net-id`). Omit for a local TwinCAT runtime. `--link twincat` only. |
| `--ams-net-id <ID>`   | AMS Net ID of the remote target, e.g. `192.168.0.1.1.1`. `--link twincat` only. |
| `--count <N>` *or* `--duration <DUR>` | Stop condition. Exactly one is required. |
| `--data-len <N>`      | Bytes of `data` per `XorHash` command. Default = 620 (Max). |
| `--sleep-ms <N>`      | Slave-side `port_sleep_ms` to inject before the response. Default = 0. |
| `--cycle-ms <N>`      | EtherCAT cycle period in milliseconds. Default = 1. |
| `--warmup <N>`        | Drop the first N samples from the summary. Default = 0. |
| `--csv <PATH>`        | Write every sample's `(index, rtt_ns, status)` to CSV. |
| `--timeout-cycles <N>`| PDO cycles to wait for an ACK match before raising `Timeout`. Default = 10. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--inflight <N>`      | Pipeline depth in `streaming` mode. Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. |
| `--low-latency`       | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path. Default: off. |

## Memory profiling

Pass `--mem-profile` to xtask to build with the `mem-profile` cargo feature, which swaps in the
[`stats_alloc`](https://crates.io/crates/stats_alloc) instrumented global allocator and appends a
process-wide allocation summary (alloc/free/realloc counts, total bytes, and per-send averages) to
the report. Use it to check whether the send hot loop allocates.

```sh
cargo xtask tool perftest --mem-profile -- --link soem --devices 1 --count 10000
```

The feature is opt-in so ordinary latency runs keep the plain system allocator and stay unperturbed.

## Modes

### `stop-and-wait` (default)

Drives `Controller::xor_hash` one command at a time, waiting for each ACK before sending the next.
Mirrors the only mode the public client API supports.
Per-sample `rtt` is the full request-response round-trip — the sum of "queue into PDI", "slave processes", "slave Rx returns", "host observes ACK" — typically 4 PDO cycles at a 1 ms cycle.
Throughput is `1 / rtt` (~ 250 cmd/s on a 1 ms cycle).

### `streaming`

Bypasses the `Controller` after the startup handshake and drives the link directly: each PDO cycle either queues a fresh `XorHash` (when the in-flight window has room) or just advances the cycle stream so the slave's `ACK` can catch up. 
Used for measuring the protocol's theoretical ceiling of one frame per cycle (~ 1000 cmd/s on a 1 ms cycle), well above what the production `Controller` API can deliver.

Per-sample `rtt` is the *individual* request's send-to-ACK latency — still ~5 cycles on a healthy link, the same as stop-and-wait.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions land one per cycle once the pipeline is primed.

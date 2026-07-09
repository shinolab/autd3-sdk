# autd3-rs-perftest

A CLI tool that streams `XorHash` commands over a real EtherCAT link and reports latency/throughput statistics.
Useful for sanity-checking the firmware ↔ host protocol, comparing kernels/NICs, and watching for regressions in `autd3-rs`'s request-response engine.

## Run

The link uses raw sockets, so it needs privileges: root or `CAP_NET_RAW` on Linux, read/write access to
`/dev/bpf*` on macOS. The easiest way is through xtask; it builds in release mode and then runs the
binary under `sudo`:

```sh
# 10,000 commands as fast as the bus allows by stop-and-wait manner
cargo xtask tool perftest -- --interface enp3s0 --count 10000

# Pipelined streaming run — measures the 1-frame-per-cycle ceiling
cargo xtask tool perftest -- --interface enp3s0 --count 10000 --mode streaming
```

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--link <KIND>`       | `ethercrab` (default), `soem`, `twincat`, or `nop`. |
| `--interface <NAME>`  | EtherCAT network interface (for `ethercrab` / `soem`). |
| `--devices <N>`       | Expected device count. Required for `twincat` (no bus scan) and `nop`; a mismatch guard otherwise. |
| `--twincat-remote <IP>` | Connect to a remote TwinCAT host over ADS (requires `--ams-net-id`). Omit for a local TwinCAT runtime. `--link twincat` only. |
| `--ams-net-id <ID>`   | AMS Net ID of the remote target, e.g. `192.168.0.1.1.1`. `--link twincat` only. |
| `--count <N>` *or* `--duration <DUR>` | Stop condition. Exactly one is required. |
| `--data-len <N>`      | Bytes of `data` per `XorHash` command. Default = 620 (Max). |
| `--sleep-ms <N>`      | Slave-side `port_sleep_ms` to inject before the response. Default = 0. |
| `--cycle-us <N>`      | EtherCAT cycle period in microseconds. Default = 1000. `0` = free-run, `--link nop` only. |
| `--warmup <N>`        | Drop the first N samples from the summary. Default = 0. |
| `--csv <PATH>`        | Write every sample's `(index, rtt_ns, status)` to CSV. |
| `--timeout-cycles <N>`| PDO cycles to wait for an ACK match before raising `Timeout`. Default = 10. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--inflight <N>`      | Pipeline depth in `streaming` mode. Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. |
| `--low-latency`       | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path. Default: off. |

## Hardware-free runs (`--link nop`)

`--link nop` swaps the EtherCAT bus for the `autd3-rs-link-nop` firmware emulator, so the whole
client stack — RT thread, slot pool, request-response engine, real CPU firmware C code — runs
without any hardware. The device count comes from `--devices` instead of a bus scan.

The emulator answers each frame instantly, so the tool paces `cycle()` itself to `--cycle-us`
(default 1000 µs) to reproduce the timing of a real bus. Pass `--cycle-us 0` to free-run: cycles
then advance as fast as the CPU allows, which turns a 10,000-sample run into a few tens of
milliseconds. Latency and throughput numbers are meaningless in free-run mode; allocation counts
are not.

```sh
# 1 ms emulated bus — throughput/latency behave like real hardware
cargo xtask tool perftest --no-sudo -- --link nop --devices 1 --duration 10s --mode streaming

# free-run — fastest way to gather allocation statistics
cargo xtask tool perftest --mem-profile --no-sudo -- --link nop --devices 1 --count 10000 --cycle-us 0
```

`--no-sudo` is worth passing: the nop link opens no raw socket, so it needs no privileges.

## Memory profiling

Pass `--mem-profile` to xtask to build with the `mem-profile` cargo feature, which installs an
instrumented global allocator and appends a process-wide allocation summary to the report:
alloc/free/realloc counts, total bytes, per-send averages, and a **histogram of allocation sizes**.

```sh
cargo xtask tool perftest --mem-profile -- --link soem --interface enp3s0 --count 10000
```

The histogram is the useful part. Counts are exact per size below 8192 bytes, sorted by the share
of bytes they contribute, so a per-send allocation shows up as a row with `per send` ≈ 1.00 and the
size tells you what it is:

```
  allocation sizes (largest share of bytes first):
        size       count    per send    bytes/send
         456       10000        1.00        456.00   <- BTreeMap<(Instant, usize), Waker> node
         104       10000        1.00        104.00
```

Recording starts after the link is open and the handshake is done, so startup allocations are
excluded; `net bytes` is therefore usually negative (frees of pre-recording allocations).

Works with every link. `--link nop --cycle-us 0` iterates fastest and needs no hardware, but only
exercises the client; use `--link ethercrab` / `--link soem` with `--interface` to profile a link
implementation against a real bus.

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

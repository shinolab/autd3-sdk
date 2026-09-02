# autd3-rs-perftest

A CLI tool that streams commands over a real EtherCAT link and reports latency/throughput statistics.

## Run

The link uses raw sockets, so it needs privileges: root or `CAP_NET_RAW` on Linux, read/write access to
`/dev/bpf*` on macOS.

```sh
# 10,000 commands as fast as the bus allows by stop-and-wait manner
cargo xtask tool perftest -- --interface enp3s0 --count 10000

# Pipelined streaming run — measures the 1-frame-per-cycle ceiling
cargo xtask tool perftest -- --interface enp3s0 --count 10000 --mode streaming

# Isolate the link path from the FPGA write
cargo xtask tool perftest -- --interface enp3s0 --count 10000 --command nop

# Allocation histogram on top of the usual summary
cargo xtask tool perftest --mem-profile -- --interface enp3s0 --count 10000
```

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--link <KIND>`       | `echocat` (default), `twincat`, `remote`, or `nop`. |
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | EtherCAT network interface (for `echocat`). Not valid with `--link nop`. |
| `--devices <N>`       | Expected device count. Required for `nop` (nothing to scan); a mismatch guard otherwise. |
| `--twincat-remote <IP>` | Connect to a remote TwinCAT host over ADS (requires `--ams-net-id`). Omit for a local TwinCAT runtime. `--link twincat` only. |
| `--ams-net-id <ID>`   | AMS Net ID of the remote target, e.g. `192.168.0.1.1.1`. `--link twincat` only. |
| `--count <N>` *or* `--duration <DUR>` | Stop condition; at most one. With neither, the run continues until Ctrl+C. |
| `--max-samples <N>`   | Cap on retained per-send samples, so an unbounded run has bounded memory. Sends continue past the cap but are no longer recorded, and the summary/CSV then cover that prefix only (a warning says so). Default = 1000000, `0` = unlimited. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--gpio-base-signal`  | Emit `BaseSignal` on GPIO[0] of every device at start-up. Probing GPIO[0] across devices with an oscilloscope shows whether they stay synchronized during the run. Default: off. |
| `--sync0-period <DUR>` | SYNC0 / EtherCAT cycle period, e.g. `1ms` / `500us` (`*LinkOption.sync0_period`). Default = `2ms` on Windows (absorbs DPC wake jitter, matching every link's own Windows default) and `1ms` elsewhere. `0ms` = free-run, `--link nop` only. |
| `--shift-percent <N>` | SYNC0 shift as a percent of the period (`*LinkOption.sync0_shift = period * percent`). Default = 0. Not valid with `--link echocat`, which keeps SYNC0 at shift 0 and phase-locks the send instant itself. |
| `--sleep-strategy <S>` | `--link echocat` only: how the RT thread waits for the next cycle (`EchocatLinkOption.sleep_strategy`). `sleep` (default) or `spin`. |
| `--spin-margin <DUR>` | How long before the deadline `--sleep-strategy spin` stops sleeping and busy-waits. Must exceed how far the OS oversleeps (0.5–0.7 ms on Windows even under `timeBeginPeriod(1)`). Default = `1ms`. |
| `--warmup <N>`        | Drop the first N samples from the summary. Default = 0. |
| `--csv <PATH>`        | Write every sample's `(index, rtt_ns, status)` to CSV. |
| `--timeout-cycles <N>`| PDO cycles to wait for an ACK match before raising `Timeout` (`ClientConfig.timeout_cycles`). Default = 10. |
| `--max-resync-rounds <N>` | Resync rounds allowed before the client gives up (`ClientConfig.max_resync_rounds`). Default = 8. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode (`ClientConfig.max_inflight`). Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. Alias: `--inflight`. |
| `--low-latency`       | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path (`ClientConfig.low_latency`). Default: off. |
| `--rt-priority <N>` / `--rt-policy <P>` / `--rt-affinity <CORE>` | RT thread scheduling (`ClientConfig.rt_priority` / `rt_policy` / `rt_affinity`). `--rt-priority` is 0..=99; omit it to keep the library default (TimeCritical on Windows, SCHED_FIFO 80 elsewhere). `--rt-affinity` alias: `--rt-core`. |
| `--no-win-perf-tune`  | Skip `PerfTuning::apply()` (the 1 ms timer resolution and HIGH process priority raised on Windows). Default: off. |

`--command` selects what is sent, which isolates where the time goes:

| `--command`            | FPGA RAM write | CTL flag latch | measures |
|------------------------|----------------|----------------|----------|
| `nop`                  | no             | no             | the link path only; the firmware acks without touching a single FPGA register |
| `write-pattern-buffer` | yes            | no             | link + FPGA RAM streaming |
| `pattern` (default)    | yes            | once per frame | the production path: the fused write+config+bank-change |

Every frame is the same size on the wire regardless of command, and the fused `pattern` is a single
frame, so all three are directly comparable frame-for-frame. The differences are the FPGA RAM write
and the per-frame latch.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt` (~ 250 cmd/s on a 1 ms cycle).

### `streaming`

Sends the selected command as fast as the link allows, without waiting for ACKs.

Used for measuring the protocol's theoretical ceiling of one frame per cycle (~ 1000 cmd/s on a 1 ms cycle).

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions land one per cycle once the pipeline is primed.

## See also

[`tools/synctune`](../synctune/README.md) measures OP-state retention and DC drift, and sweeps
`sync0_period` / `sync0_shift` for the setting that holds OP. Pick the timing there first, then
load-test it here.

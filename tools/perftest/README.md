# autd3-rs-perftest

A CLI tool that streams commands to AUTD3 devices over UDP and reports latency/throughput statistics.

## Run

The client uses ordinary UDP sockets on IPv6 link-local addresses, so it needs neither root nor an IP address
on the host NIC. The host firewall must accept UDP from `fe80::/10` port 44336 on that interface.

```sh
# 10,000 commands as fast as the chain allows by stop-and-wait manner
cargo xtask tool perftest -- --devices 2 --count 10000

# Pipelined streaming run — keeps up to `--max-inflight` frames in flight
cargo xtask tool perftest -- --devices 2 --count 10000 --mode streaming

# Isolate the communication path from the FPGA write
cargo xtask tool perftest -- --devices 2 --count 10000 --command nop

# Host-side baseline against the in-process device emulator (no hardware)
cargo xtask tool perftest -- --devices 2 --count 10000 --emulator

# Against the simulator
cargo xtask tool perftest -- --devices 1 --count 1000 --group '[::1]:44336'

# Allocation histogram on top of the usual summary
cargo xtask tool perftest --mem-profile -- --devices 2 --count 10000
```

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--emulator`          | Run against the in-process UDP device emulator (firmware emulator behind real UDP sockets on `::1`) instead of real devices. Default: off. |
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, `write-modulation-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | Network interface the devices hang off (`TransportOption.iface`). Omit to pick the one whose devices answer. |
| `--group <ADDR>`      | Send the multicast management messages to this address instead of `ff02::1` (`TransportOption.group`), e.g. the simulator's `[::1]:44336`. |
| `--devices <N>`       | Device count of the geometry. Default = 1. Opening fails when it does not match the chain. |
| `--heartbeat <DUR>`   | Heartbeat interval while nothing is sent (`TransportOption.heartbeat`). Default = `10ms`. |
| `--reply-timeout <DUR>` | How long a heartbeat waits for every reply (`TransportOption.reply_timeout`). Omit to keep the library default. |
| `--count <N>` *or* `--duration <DUR>` | Stop condition; at most one. With neither, the run continues until Ctrl+C. |
| `--max-samples <N>`   | Cap on retained per-send samples, so an unbounded run has bounded memory. Sends continue past the cap but are no longer recorded, and the summary/CSV then cover that prefix only (a warning says so). Default = 1000000, `0` = unlimited. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--gpio-base-signal`  | Emit `BaseSignal` on GPIO[0] of every device at start-up. Probing GPIO[0] across devices with an oscilloscope shows whether they stay synchronized during the run. Default: off. |
| `--gpio-sync`         | Emit `Sync` (high on the FPGA clock that detects the sync input edge) on GPIO[0] of every device at start-up, to probe the sync pulse itself. Mutually exclusive with `--gpio-base-signal`. Default: off. |
| `--telemetry`         | Read the firmware telemetry counters before and after the run and print the per-device deltas (e.g. `Failsafe`, `SyncResync`, `FifoDrop`). Default: off. |
| `--hold <DUR>`        | Keep the connection idle (heartbeats only) this long before measuring, e.g. `3s`. |
| `--warmup <N>`        | Drop the first N samples from the summary. Default = 0. |
| `--csv <PATH>`        | Write every sample's `(index, rtt_ns, status)` to CSV. |
| `--ack-timeout <DUR>` | How long a request waits for its ACK before raising `Timeout` (`ClientConfig.ack_timeout`). Default = `10ms`. |
| `--max-resync-rounds <N>` | Resync rounds allowed before the client gives up (`ClientConfig.max_resync_rounds`). Default = 8. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode (`ClientConfig.max_inflight`). Default = 7 (`DEVICE_QUEUE_FRAMES`, the device's receive queue). `stop-and-wait` ignores it except with `--command write-modulation-buffer`, which pipelines the frames of one sample up to this depth. Alias: `--inflight`. |

`--command` selects what is sent, which isolates where the time goes:

| `--command`            | FPGA RAM write | CTL flag latch | measures |
|------------------------|----------------|----------------|----------|
| `nop`                  | no             | no             | the communication path only; the firmware acks without touching a single FPGA register |
| `write-pattern-buffer` | yes            | no             | communication + FPGA RAM streaming |
| `pattern` (default)    | yes            | once per pattern | the production path: write + config + bank-activation, 3 frames |
| `write-modulation-buffer` | yes (modulation buffer of bank 1) | no | bulk transfer: one sample writes the whole modulation buffer, its frames pipelined up to `--max-inflight` |

`nop` and `write-pattern-buffer` are one frame per sample. `write-modulation-buffer` is one whole-buffer write per sample (many frames), and runs in `stop-and-wait` only. `pattern` is three frames (write, config,
bank change), of which only the bank change latches: in `stop-and-wait` one sample is
the whole pattern (three round trips), in `streaming` every frame is its own sample, so `--count`
counts frames and three of them make one pattern.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt`.

### `streaming`

Sends the selected command as fast as the chain allows, without waiting for ACKs.

Measures the throughput ceiling with up to `--max-inflight` frames in flight.

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions arrive back to back once the pipeline is primed.

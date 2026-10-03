# autd3-rs-perftest

A CLI tool that streams commands to AUTD3 devices over UDP and reports latency/throughput statistics.

## Run

The client uses ordinary UDP sockets on IPv6 link-local addresses, so it needs neither root nor an IP address
on the host NIC. The host firewall must accept UDP from `fe80::/10` port 44336 on that interface.

```sh
# 10,000 commands as fast as the chain allows by stop-and-wait manner
cargo xtask tool perftest -- --devices 2 --count 10000

# Pipelined streaming run — measures the 1-frame-per-cycle ceiling
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
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | Network interface the devices hang off (`TransportOption.iface`). Omit to pick the one whose devices answer. |
| `--group <ADDR>`      | Send the multicast management messages to this address instead of `ff02::1` (`TransportOption.group`), e.g. the simulator's `[::1]:44336`. |
| `--devices <N>`       | Device count of the geometry. Default = 1. Opening fails when it does not match the chain. |
| `--reply-timeout <DUR>` | How long a cycle waits for every reply (`TransportOption.reply_timeout`). Omit to keep the library default. |
| `--count <N>` *or* `--duration <DUR>` | Stop condition; at most one. With neither, the run continues until Ctrl+C. |
| `--max-samples <N>`   | Cap on retained per-send samples, so an unbounded run has bounded memory. Sends continue past the cap but are no longer recorded, and the summary/CSV then cover that prefix only (a warning says so). Default = 1000000, `0` = unlimited. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--gpio-base-signal`  | Emit `BaseSignal` on GPIO[0] of every device at start-up. Probing GPIO[0] across devices with an oscilloscope shows whether they stay synchronized during the run. Default: off. |
| `--cycle <DUR>`       | Send period, e.g. `1ms` / `500us` (`TransportOption.cycle`). Default = `1ms`. |
| `--warmup <N>`        | Drop the first N samples from the summary. Default = 0. |
| `--csv <PATH>`        | Write every sample's `(index, rtt_ns, status)` to CSV. |
| `--timeout-cycles <N>`| Cycles to wait for an ACK match before raising `Timeout` (`ClientConfig.timeout_cycles`). Default = 10. |
| `--max-resync-rounds <N>` | Resync rounds allowed before the client gives up (`ClientConfig.max_resync_rounds`). Default = 8. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode (`ClientConfig.max_inflight`). Default = 127 (the SEQ-wrap cap). Ignored in `stop-and-wait`. Alias: `--inflight`. |
| `--low-latency`       | Request the device's low-latency (inline ISR) processing mode instead of the default FIFO path (`ClientConfig.low_latency`). Default: off. |
| `--rt-priority <N>` / `--rt-policy <P>` / `--rt-affinity <CORE>` | Scheduling of the thread perftest spawns to run the `Driver` (the library itself spawns no thread). `--rt-priority` is 0..=99 and `--rt-policy` (default `fifo`, Linux only) applies only with it; omit both to run the driver at the normal priority, as an application does by default. `--rt-affinity` alias: `--rt-core`. |
| `--poll-sleep <DUR>`  | Run the driver as a `poll` loop that sleeps `DUR` between polls instead of `Driver::run`, e.g. `1ms`. Latency grows by up to `DUR`; retransmissions and lost replies must stay 0. Default: off. |

`--command` selects what is sent, which isolates where the time goes:

| `--command`            | FPGA RAM write | CTL flag latch | measures |
|------------------------|----------------|----------------|----------|
| `nop`                  | no             | no             | the communication path only; the firmware acks without touching a single FPGA register |
| `write-pattern-buffer` | yes            | no             | communication + FPGA RAM streaming |
| `pattern` (default)    | yes            | once per pattern | the production path: write + config + bank-activation, 3 frames |

`nop` and `write-pattern-buffer` are one frame per sample. `pattern` is three frames (write, config,
bank change), of which only the bank change latches: in `stop-and-wait` one sample is
the whole pattern (three round trips), in `streaming` every frame is its own sample, so `--count`
counts frames and three of them make one pattern.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt` (~ 250 cmd/s on a 1 ms cycle).

### `streaming`

Sends the selected command as fast as the chain allows, without waiting for ACKs.

Used for measuring the protocol's theoretical ceiling of one frame per cycle (~ 1000 cmd/s on a 1 ms cycle).

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions land one per cycle once the pipeline is primed.

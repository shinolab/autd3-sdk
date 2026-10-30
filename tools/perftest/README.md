# autd3-rs-perftest

A CLI tool that streams commands to AUTD3 devices over UDP and reports latency/throughput statistics.

## Run

The client uses ordinary UDP sockets on IPv6 link-local addresses, so it needs neither root nor an IP address
on the host NIC. The host firewall must accept UDP from `fe80::/10` port 44336 on that interface.

```sh
# One command at a time (stop-and-wait), until Ctrl+C
cargo xtask tool perftest -- --devices 2

# Pipelined streaming run — keeps up to `--max-inflight` frames in flight
cargo xtask tool perftest -- --devices 2 --mode streaming

# Isolate the communication path from the FPGA write
cargo xtask tool perftest -- --devices 2 --command nop
```

The run continues until Ctrl+C, then prints the summary.
Besides the per-sample latency it lists what perftest itself counted while measuring: the frames it sent, the heartbeats `--hold` sent, and the ack latency of every frame (mean and max).
The ack latency runs from the `send_frame_at` call to the moment its `ResponseFuture` resolves, so it includes the wake-up of the awaiting task, not only the wire.

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | Network interface the devices hang off (`TransportOption.iface`). Omit to pick the one whose devices answer. |
| `--devices <N>`       | Device count of the geometry. Default = 1. Opening fails when it does not match the chain. |
| `--heartbeat <DUR>`   | Interval of `Client::send_heartbeat` while `--hold` keeps the connection idle. Default = `10ms`. The library sends no heartbeat by itself. |
| `--send-buffer <BYTES>` | Socket send buffer (`TransportOption.send_buffer_bytes`). `0` keeps the OS default. Omit to use the library default. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--hold <DUR>`        | Keep the connection idle (heartbeats every `--heartbeat` only) this long before measuring, e.g. `3s`. |
| `--ack-timeout <DUR>` | How long a request waits for its ACK before raising `Timeout` (`TransportOption.ack_timeout`). Default = `10ms`. Must exceed `--exec-lead` (plus the processing time). |
| `--exec-lead <DUR>`   | Stamp every frame with `EXEC_AT` = the device time this far ahead, rounded up to 1 µs (each frame is sent with `send_frame_at(frame, ExecAt::Lead(..))`), e.g. `1ms`. Omit to process every frame on arrival. |
| `--mode <MODE>`       | `stop-and-wait` (default) or `streaming`. See below. |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode: frames sent before awaiting the oldest response. Default = 7 (`MAX_INFLIGHT`, the device's receive queue). `stop-and-wait` ignores it. Alias: `--inflight`. |

`--command` selects what is sent, which isolates where the time goes:

| `--command`            | FPGA RAM write | CTL flag latch | measures |
|------------------------|----------------|----------------|----------|
| `nop`                  | no             | no             | the communication path only; the firmware acks without touching a single FPGA register |
| `write-pattern-buffer` | yes            | no             | communication + FPGA RAM streaming |
| `pattern` (default)    | yes            | once per pattern | the production path: write + config + bank-activation fused into 1 frame (`WritePatternFused`) |

Every command is one frame per sample.
`pattern` writes the RAM, the bank config and the bank change in one frame, which latches once.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt`.

### `streaming`

Sends the selected command as fast as the chain allows, without waiting for ACKs.

Measures the throughput ceiling with up to `--max-inflight` frames in flight.

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions arrive back to back once the pipeline is primed.

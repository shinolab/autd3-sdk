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

# Host-side baseline against the in-process device emulator (no hardware)
cargo xtask tool perftest -- --devices 2 --emulator

# Against the simulator
cargo xtask tool perftest -- --devices 1 --simulator

# Allocation histogram on top of the usual summary
cargo xtask tool perftest --mem-profile -- --devices 2

# Bank-switch delay and inter-device spread, 5 ms exec_lead, Pattern as 3 pipelined frames
cargo xtask tool perftest -- --devices 4 --mode bank-switch --exec-lead 5ms --framing split
```

The run continues until Ctrl+C, then prints the summary.

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--emulator`          | Run against the in-process UDP device emulator (firmware emulator behind real UDP sockets on `::1`) instead of real devices. Default: off. |
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, `write-modulation-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | Network interface the devices hang off (`TransportOption.iface`). Omit to pick the one whose devices answer. |
| `--simulator [PORT]`  | Connect to the simulator on this host only, listening on `PORT` (default 44336; `Interface::Simulator(PORT)`). Without it `Interface::Auto` fails when both the simulator and devices answer. |
| `--devices <N>`       | Device count of the geometry. Default = 1. Opening fails when it does not match the chain. |
| `--heartbeat <DUR>`   | Interval of `Client::send_heartbeat` while `--hold` keeps the connection idle. Default = `10ms`. The library sends no heartbeat by itself. |
| `--send-rate-limit <PERCENT>` | Cap on the line occupancy of the sends in percent of the device link speed (`TransportOption.send_rate_limit_percent`). Omit to send without a limit. |
| `--send-buffer <BYTES>` | Socket send buffer (`TransportOption.send_buffer_bytes`). `0` keeps the OS default. Omit to use the library default. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--hold <DUR>`        | Keep the connection idle (heartbeats every `--heartbeat` only) this long before measuring, e.g. `3s`. |
| `--ack-timeout <DUR>` | How long a request waits for its ACK before raising `Timeout` (`TransportOption.ack_timeout`). Default = `10ms`. Must exceed `--exec-lead` (plus the processing time). |
| `--exec-lead <DUR>`   | Stamp every frame with `EXEC_AT` = the device time this far ahead, rounded up to 1 µs (each frame is sent with `send_frame_at(frame, ExecAt::Lead(..))`), e.g. `1ms`. At least `100us`. Omit to process every frame on arrival. |
| `--mode <MODE>`       | `stop-and-wait` (default), `streaming`, or `bank-switch`. See below. |
| `--framing <FRAMING>` | `bank-switch` only: `fused` (default; `Pattern`, 1 `WritePatternFused` frame) or `split` (`WritePatternBuffer` + `ConfigPattern` + `ActivatePatternBank`, 3 frames pipelined). |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode: frames sent before awaiting the oldest response. Default = 7 (`DEVICE_QUEUE_FRAMES`, the device's receive queue). `stop-and-wait` ignores it except with `--command write-modulation-buffer`, which pipelines the frames of one sample up to this depth. Alias: `--inflight`. |

`--command` selects what is sent, which isolates where the time goes:

| `--command`            | FPGA RAM write | CTL flag latch | measures |
|------------------------|----------------|----------------|----------|
| `nop`                  | no             | no             | the communication path only; the firmware acks without touching a single FPGA register |
| `write-pattern-buffer` | yes            | no             | communication + FPGA RAM streaming |
| `pattern` (default)    | yes            | once per pattern | the production path: write + config + bank-activation fused into 1 frame (`WritePatternFused`) |
| `write-modulation-buffer` | yes (modulation buffer of bank 1) | no | bulk transfer: one sample writes the whole modulation buffer, its frames pipelined up to `--max-inflight` |

`nop`, `write-pattern-buffer` and `pattern` are one frame per sample.
`pattern` writes the RAM, the bank config and the bank change in one frame, which latches once.
`write-modulation-buffer` is one whole-buffer write per sample (many frames), and runs in `stop-and-wait` only.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt`.

### `streaming`

Sends the selected command as fast as the chain allows, without waiting for ACKs.

Measures the throughput ceiling with up to `--max-inflight` frames in flight.

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions arrive back to back once the pipeline is primed.

### `bank-switch`

Alternates zero-amplitude Patterns between bank B0 and B1, waiting for every reply of one switch before the next, with GPIO[0] on `PatternBank`, so the switch can be probed on two or more devices.
It sends `Clear` first, so the pulse width table is the default one, whose entry for amplitude 0 is 0: no transducer emits.
It then sends `Synchronize`, as the client modes do right after opening: opening the bus restarts PTP, and without it the FPGA clocks of the devices keep the offsets they had before, so the edges of different devices would land on different 25 µs grids.

It drives `UdpBus` directly instead of the `Client` to read the device time each device puts in its reply (`FrameReply.sys_time`, taken right after the frame was processed).
It stamps `EXEC_AT` the way `ExecAt::Lead` does (per frame, from the device time right before that frame is sent). A failed switch is counted and the next switch is sent as is.
`--command`, `--max-inflight` and `--hold` do not apply.

Printed on Ctrl+C:

| block | value |
|-------|-------|
| `delay` | per device: reply device time of the switching frame (the last one with `--framing split`) - device time right before the first frame is sent |
| `after the boundary` | per device, with `--exec-lead`: reply device time - `EXEC_AT` of the switching frame |
| `spread` | per switch: latest - earliest reply device time across the devices |
| `telemetry` | per device delta of `exec_late`, `fifo_drop`, `dispatch_error`, `send_failure`, `failsafe`; `sync_resync` at the end |

Values are binned at 1 µs between -10 ms and `--ack-timeout` + 10 ms; min and max are exact.
`--ack-timeout` must be shorter than the lost timeout, since this mode neither sends heartbeats nor brings Lost devices back.
On an error the exchange cannot report as a failed switch (or the first failure with `--stop-on-error`) the run stops, prints the summary, and names the Lost devices.
The in-process emulator ignores `EXEC_AT`, so `after the boundary` is meaningful only on real devices.

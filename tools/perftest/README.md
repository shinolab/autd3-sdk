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
```

The run continues until Ctrl+C, then prints the summary.

## Arguments

| Flag                  | Description |
|-----------------------|-------------|
| `--emulator`          | Run against the in-process UDP device emulator (firmware emulator behind real UDP sockets on `::1`) instead of real devices. Default: off. |
| `--command <CMD>`     | `pattern` (default), `write-pattern-buffer`, `write-modulation-buffer`, or `nop`. See the table below. |
| `--interface <NAME>`  | Network interface the devices hang off (`UdpOption.iface`). Omit to pick the one whose devices answer. |
| `--simulator [PORT]`  | Connect to the simulator on this host only, listening on `PORT` (default 44336; `Interface::Simulator(PORT)`). Without it `Interface::Auto` fails when both the simulator and devices answer. |
| `--server <PORT>`     | Connect through the `autd3-server` on this loopback port (`ServerLink`). The UDP options come from the server, so `--heartbeat` / `--reply-timeout` / `--send-rate-limit` / `--send-buffer` are ignored. |
| `--devices <N>`       | Device count of the geometry. Default = 1. Opening fails when it does not match the chain. |
| `--heartbeat <DUR>`   | Heartbeat interval while nothing is sent (`UdpOption.heartbeat`). Default = `10ms`. |
| `--reply-timeout <DUR>` | How long a heartbeat waits for every reply (`UdpOption.reply_timeout`). Omit to keep the library default. |
| `--send-rate-limit <PERCENT>` | Cap on the line occupancy of the sends in percent of the device link speed (`UdpOption.send_rate_limit`). Omit to send without a limit. |
| `--send-buffer <BYTES>` | Socket send buffer (`UdpOption.send_buffer`). `0` keeps the OS default. Omit to use the library default. |
| `--stop-on-error`     | Stop at the first failed send and exit non-zero. The summary is still printed. Default: off. |
| `--hold <DUR>`        | Keep the connection idle (heartbeats only) this long before measuring, e.g. `3s`. |
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

`nop` and `write-pattern-buffer` are one frame per sample.
`write-modulation-buffer` is one whole-buffer write per sample (many frames), and runs in `stop-and-wait` only.
`pattern` is three frames (write, config, bank change), of which only the bank change latches:
in `stop-and-wait` one sample is the whole pattern (three round trips), in `streaming` every frame is its own sample, and three of them make one pattern.

## Modes

### `stop-and-wait` (default)

Sends the selected command one at a time, waiting for each ACK before sending the next.

Throughput is `1 / rtt`.

### `streaming`

Sends the selected command as fast as the chain allows, without waiting for ACKs.

Measures the throughput ceiling with up to `--max-inflight` frames in flight.

Per-sample `rtt` is the *individual* request's send-to-ACK latency.
The difference shows up in throughput, not latency: many requests are in flight at once, so completions arrive back to back once the pipeline is primed.

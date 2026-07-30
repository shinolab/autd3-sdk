# autd3-rs-synctune

A CLI tool that measures how well a real EtherCAT link holds OP state under load, and sweeps `sync0_period` / `sync0_shift` to find the setting that holds it best.

## Run

The link uses raw sockets, so it needs privileges: root or `CAP_NET_RAW` on Linux, read/write access to
`/dev/bpf*` on macOS.

```sh
# One configuration, 30 s of load: OP retention, drops, throughput, DC drift
cargo xtask tool synctune -- measure --interface enp3s0

# Sweep period 1ms..2ms x shift 0..100% and report the best candidate
cargo xtask tool synctune -- tune --link ethercrab --interface enp3s0

# Long single-configuration run for the bus-clock vs host-clock rate only
cargo xtask tool synctune -- drift --interface enp3s0 --duration 300s
```

## Subcommands

| Subcommand | What it does |
|------------|--------------|
| `measure`  | Runs one `(sync0_period, shift_percent)` for `--warmup` + `--dwell` and reports OP retention, degraded-state breakdown, drop/lost/recovery events, throughput, and DC drift. |
| `tune`     | Sweeps the period x shift grid, runs `measure` on each candidate, prints a table, and marks the best one (tie-break: higher OP ratio, fewer drops, lower shift, lower period). |
| `drift`    | One long run focused on the bus DC time vs the host wall clock: linear-regression rate in ppm, regression residual, and extrapolated time until the two are 10 ms / 1 s apart. `--duration` overrides `--dwell`. |

## Common arguments

Shared by all three subcommands.

| Flag                  | Description |
|-----------------------|-------------|
| `--link <KIND>`       | `echocat` (default), `ethercrab`, or `soem`. |
| `--interface <NAME>`  | EtherCAT network interface (`*LinkOption.iface`). |
| `--devices <N>`       | Expected device count; a mismatch aborts the run. |
| `--mode <MODE>`       | Load pattern: `streaming` (default) or `stop-and-wait`. Streaming keeps the bus at its one-frame-per-cycle ceiling, which is the condition drops show up under. |
| `--max-inflight <N>`  | Pipeline depth in `streaming` mode (`ClientConfig.max_inflight`). Default = 127 (the SEQ-wrap cap). Alias: `--inflight`. |
| `--timeout-cycles <N>`| PDO cycles to wait for an ACK match before raising `Timeout` (`ClientConfig.timeout_cycles`). Default = 10. |
| `--max-resync-rounds <N>` | Resync rounds allowed before the client gives up (`ClientConfig.max_resync_rounds`). Default = 8. |
| `--low-latency`       | Request the slave's low-latency (inline ISR) processing mode instead of the default FIFO path (`ClientConfig.low_latency`). Default: off. |
| `--rt-priority <N>` / `--rt-policy <P>` / `--rt-affinity <CORE>` | RT thread scheduling (`ClientConfig.rt_priority` / `rt_policy` / `rt_affinity`). `--rt-priority` is 0..=99; omit it to keep the library default (TimeCritical on Windows, SCHED_FIFO 80 elsewhere). `--rt-affinity` alias: `--rt-core`. |
| `--no-rt-priority`    | Force `rt_priority = None` (no RT scheduling), overriding the library default. Use it to compare against the pre-default behaviour. Conflicts with `--rt-priority`. |
| `--tx-rx-priority <N>` / `--tx-rx-policy <P>` / `--tx-rx-affinity <CORE>` | `--link ethercrab` only: the tx/rx pump thread (`EtherCrabLinkOptionFull.tx_rx_*`). Omit `--tx-rx-priority` to keep the library default (90 outside Windows). |
| `--no-tx-rx-priority` | `--link ethercrab` only: leave the pump thread at the OS default. Conflicts with `--tx-rx-priority`. |
| `--dwell <DUR>`       | Measurement window per candidate. Default = `30s`. |
| `--warmup <DUR>`      | Time excluded from the statistics at the start of each candidate. Default = `5s`. |
| `--poll-interval <DUR>` | How often the AL state and DC time are sampled. Default = `100ms`. |
| `--csv <PATH>`        | Write one row per candidate (period/shift, retention, events, throughput, drift) to CSV. |
| `--no-win-perf-tune`  | Skip `PerfTuning::apply()` (the 1 ms timer resolution and HIGH process priority raised on Windows). Default: off. |

## Subcommand arguments

| Subcommand | Flag | Description |
|------------|------|-------------|
| `measure` / `drift` | `--sync0-period <DUR>` | SYNC0 / EtherCAT cycle period, e.g. `1ms` / `500us` (`*LinkOption.sync0_period`). Default = `1ms`. |
| `measure` / `drift` | `--shift-percent <N>` | SYNC0 shift as a percent of the period (`*LinkOption.sync0_shift = period * percent`). Default = 0. |
| `drift`    | `--duration <DUR>` | Sampling window; overrides `--dwell`. Longer windows tighten the ppm estimate. Default = `120s`. |
| `tune`     | `--period-min` / `--period-max` / `--period-step` | Period grid. Defaults = `1ms` / `2ms` / `1ms`. |
| `tune`     | `--shift-min` / `--shift-max` / `--shift-step` | Shift grid in percent. Defaults = 0 / 100 / 50. |

A SYNC0 shift is not valid with `--link echocat`: it keeps SYNC0 at shift 0 and phase-locks the send instant on its own.
Use `--shift-max 0` when sweeping over echocat.

The total run time of `tune` is `candidates * (--warmup + --dwell)`.

## Reading the results

| Field | Meaning |
|-------|---------|
| `OP retention` | Fraction of polls where every device was in OP. This is the number the sweep optimizes. |
| `degraded`     | Breakdown of the non-OP polls: `safe-op` / `safe-op-err` / `lost` / `other`. |
| `events`       | `drops` = OP → non-OP transitions, `lost` = devices dropping off the bus, `recoveries` = link-level recoveries, `first-drop` = time from the end of warmup to the first drop. |
| `throughput`   | Successful frames per second (and MB/s) over the measured window. |
| `dc drift`     | Bus DC time vs the host clock, in ppm, with the extrapolated time until they are 10 ms apart. |

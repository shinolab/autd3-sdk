# autd3-rs-recipes

Rust recipes that do not fit in the documentation. Each recipe is a binary under `src/bin/<name>/`.

```sh
cargo xtask rust recipe <name> [--sudo] -- <args>
```

## `cyclic`

Makes every device process one frame per fixed period.
`CyclicController` (`src/bin/cyclic/controller.rs`) wraps a `Controller`: a dedicated thread sends the frame of cycle `k` with `ExecAt::At(t0 + k * period)`,
`lead` ahead of that device time, and fills idle cycles with `Nop`. `send(cmd)` queues the frames of a command one per cycle and waits for the replies of each.

The binary keeps sending a zero-intensity `Pattern` through `send` and prints, every second and on Ctrl+C,
how late the host sent, how long after `EXEC_AT` each device finished, how far apart the devices finished, the smallest arrival margin before `EXEC_AT`,
missing replies and the `exec_late` / `fifo_drop` increments.

It raises the calling thread to `SCHED_FIFO` 80 on Linux (needs root or an `rtprio` limit, hence `--sudo`), and to `TimeCritical` on Windows through `thread-priority`. macOS keeps the normal priority. It needs real devices (emulators ignore `EXEC_AT`).

```sh
cargo xtask rust recipe cyclic --sudo -- --interface eth0 --devices 2 --period 1ms --lead 2ms
```

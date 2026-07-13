# autd3-rs CPU firmware

Firmware for the next-generation AUTD3 CPU board (RZ/T1 R7S910018, Cortex-R4F). `no_std` Rust.

## Layout

| Path        | Crate          | What it is |
|-------------|----------------|------------|
| `fw/`       | `autd3-cpu-fw` | Portable firmware logic (protocol, FPGA access, command handlers). Hardware sits behind the `Port` trait, so the same code runs on the board, under the host unit tests and inside `autd3-rs-firmware-emulator`. Member of the main workspace, but **`publish = false`**: the emulator does not depend on it, it compiles a vendored copy of these sources (`cargo xtask vendor-fw`), so that the emulator stays publishable to crates.io. |
| `board/`    | `autd3-cpu`    | The board target: RZ/T1 registers, BSP (clock/bus/io/vic/timer), the `Port` implementation and the C-ABI symbols `platform.o` calls (`init_app` / `recv_ethercat` / `app_process_pending` / `bsp_*`). Builds as a `staticlib` for `armv7r-none-eabi`; excluded from the workspace (it needs `panic = "abort"` and a bare-metal target). |
| `platform/` | —              | The prebuilt proprietary boot layer (`autd3-platform.o`: Renesas loader/startup, serial flash, EtherCAT host interface, `main`) plus the linker script. Built and exported from the private `autd3-cpu-internal` repository. |
| `gen_param.py` | —           | Generates `fw/src/params.rs` from the FPGA `params.svh`, which is the single source of truth for the hardware constants. |

`platform.o` owns `main()` and calls into the Rust firmware; `recv_ethercat` runs in the EtherCAT host-interface ISR while `app_process_pending` runs in the main loop.

## Build & test

Run development tasks via the repository's **`cargo xtask`** (from within `autd3-sdk/`):

```bash
cargo xtask cpu build    # cargo build (staticlib) + link with platform.o -> build/autd3-cpu.bin
cargo xtask cpu test     # host unit tests of the firmware logic
cargo xtask cpu lint
cargo xtask cpu format
```

`cpu build` needs the bare-metal target (`rustup target add armv7r-none-eabi`) and `arm-none-eabi-gcc`, which performs the final link (set `CROSS_COMPILE` to override the prefix).

## Opening in e2 studio

`.project` / `.cproject` let you open this directory in e2 studio (Renesas' Eclipse-based IDE) for J-Link debugging. The project does **not** define its own toolchain: `Build Project` just runs `cargo xtask cpu build` as an external builder (working directory `../..`, i.e. `autd3-sdk/`), so the build stays defined in one place (`xtask/src/cpu.rs`).

Import with `File > Open Projects from File System...` and point it at `firmware/cpu`.

### J-Link debugging

`autd3-cpu-jlink.launch` is a shared *GDB Hardware Debugging* configuration for a Segger J-Link.

**Read this first — you cannot single-step the live EtherCAT path.** The RZ/T1's EtherCAT Slave Controller emulates its SII/EEPROM in the CPU firmware (see `ECATC.CATEMMD` setup and the host-interface ISR, which lives in the proprietary `platform/autd3-platform.o`). The EtherCAT master reads the SII device name (`"AUTD"`) during bus enumeration; if the CPU is halted or reset, the emulation stops answering and the client fails with `link error: No AUTD device found`. Consequently:

- **The launch does not reset the chip** (`doReset=false`) and does not halt on connect (`doHalt=false`). A reset would clear the DC registers the master programmed (including `0x09A0`) and tear down enumeration.
- **Halting the core drops the EtherCAT link.** Any breakpoint that fires while the link must stay live stalls the EEPROM/SM emulation and the master drops the slave. So this configuration is for *attaching to an already-running, already-enumerated device and inspecting state*, not for stepping through the cyclic path.

For debugging firmware **logic** (command handlers, validation, etc.), use `autd3-rs-firmware-emulator` instead: it compiles the real firmware sources (`fw/src`) into itself and runs on the host, where you can set ordinary breakpoints with no EtherCAT timing to honour.

The linker script (`platform/autd3-cpu.ld`) places `.text` / `.rodata` / `.data` at VMA `0x00040000` (ATCM) with LMA in serial flash, and a loader copies them at reset, so the debug session **loads symbols only** (`build/autd3-cpu.x`), never the image. Because there is no reset, the running firmware's addresses already match the ELF VMA.

Workflow (e.g. to read the SYNC0 cycle time the master programmed):

1. `cargo xtask cpu flash` — flash the board.
2. Start the GDB server (leave it running):
   ```bash
   JLinkGDBServerCLExe -device R7S910018_R4F -if JTAG -speed 4000 -jtagconf -1,-1 -endian little -port 2331
   ```
   The device / interface / speed / `-jtagconf` match `cargo xtask cpu flash` (`xtask/src/cpu.rs`); keep them in sync if either changes.
3. Open the EtherCAT link from the host (e.g. run an example) so the device reaches OP and the master programs the DC registers.
4. Launch the `autd3-cpu-jlink` configuration, then **Suspend** to halt. The link will drop, but registers the master already latched keep their values. The Memory view is greyed out until the target is suspended.
5. In the Memory view, add a monitor (the green **+**) for `0xA00D09A0` and read 4 bytes little-endian: that is the SYNC0 cycle time in ns (e.g. `--sync0 2` = 1 ms = `0x000F4240`). Related registers: `0xA00D0910` (DC system time), `0xA00D0990` (cyclic start time).

The Debugger tab must connect to the **external** GDB server, so the JTAG Device is **Generic TCP/IP** (not "SEGGER J-Link", which tries to spawn its own server), with *Use remote target* checked, Host name `localhost`, Port `2331`. e2 studio re-normalizes the `.launch` when you open it, so verify these in the UI if the committed file gets rewritten.

If launching fails with `-target-select remote 2331` / `could not open device: No such file or directory`, the host name was lost and gdb treated `2331` as a serial device: set JTAG Device to Generic TCP/IP and Host name to `localhost` so gdb runs `target remote localhost:2331`.

`cargo xtask cpu build` builds the `release` profile (`opt-level = "s"`, LTO), so `build/autd3-cpu.x` carries symbols (function/variable addresses) but no line tables: symbol lookup, register and memory inspection work, source-line stepping does not. That is enough for the register-inspection workflow above. For source-line debugging, add `debug = true` to `[profile.release]` in `board/Cargo.toml` for a local build (it does not change the flashed `.bin`, which `objcopy` strips) — but remember the halt still drops the link.

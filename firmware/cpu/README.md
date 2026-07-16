# AUTD3 CPU Firmware

## Structure

```
firmware/cpu/
├── README.md
├── fw/                   # [crate autd3-cpu-fw] portable firmware logic (protocol, FPGA
│                         #   access, command handlers). Hardware sits behind the `Port` trait,
│                         #   so the same code runs on the board, the host unit tests and the
│                         #   emulator. `publish = false`; the emulator vendors these sources.
├── board/                # [crate autd3-cpu] the board target: RZ/T1 registers, BSP, the `Port`
│                         #   implementation and the C-ABI symbols platform.o calls. staticlib
│                         #   for armv7r-none-eabi (excluded from the workspace).
├── platform/             # prebuilt proprietary boot layer (autd3-platform.o: Renesas loader,
│                         #   serial flash, EtherCAT host interface, main) + the linker script.
├── .project / .cproject / autd3-cpu-jlink.launch   # e2 studio project + J-Link debug config
└── build/                # generated .bin / .x / .map (git-ignored)
```

## Build & test

All tasks are run from the repository root (`autd3-sdk/`) via xtask.

```bash
cargo xtask cpu build          # cargo build (staticlib) + link with platform.o -> build/autd3-cpu.bin
cargo xtask cpu flash          # build, then write autd3-cpu.bin to the device with J-Link
cargo xtask cpu test           # host unit tests of the portable firmware logic
cargo xtask cpu lint
cargo xtask cpu format         # add --fix to rewrite instead of only checking
```

## Code generation

```bash
cargo xtask cpu gen-param      # regenerate fw/src/params.rs from the FPGA params.svh
```

# Author

Shun Suzuki, 2022-2026

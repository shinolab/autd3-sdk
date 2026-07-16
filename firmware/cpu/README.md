# AUTD3 CPU Firmware

## Structure

```
firmware/cpu/
├── README.md
├── wire/                 # shared wire-protocol contract
├── fw/                   # portable firmware logic
├── board/                # the board target: RZ/T1 registers, BSP, the `Port` implementation and the C-ABI symbols platform.o calls
└── platform/             # prebuilt proprietary boot layer
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

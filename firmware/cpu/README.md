# AUTD3 CPU Firmware

## Structure

```
firmware/cpu/
├── wire/                 # shared wire-protocol contract
├── fw/                   # portable firmware logic
├── board/                # the board target: RZ/T1 registers, BSP, startup, serial flash driver, the `Port` implementation and the loader (`src/bin/loader.rs`)
└── platform/             # linker scripts for the application and the loader
```

## Build & test

```bash
cargo xtask cpu build          # cargo build (staticlib + loader), link, merge -> build/autd3-cpu.bin
cargo xtask cpu flash          # build, then write autd3-cpu.bin to the device with J-Link
cargo xtask cpu test           # host unit tests of the portable firmware logic
cargo xtask cpu lint
cargo xtask cpu format         # add --fix to rewrite instead of only checking
```

## Code generation

```bash
cargo xtask cpu gen-param      # regenerate fw/src/fpga_params.rs from the FPGA params.svh
```

# Author

Shun Suzuki, 2022-2026

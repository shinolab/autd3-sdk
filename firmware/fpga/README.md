# AUTD3-FPGA

Version: 0.1.0

## Build

All Vivado tasks are run from the repository root (`autd3-sdk/`) via xtask.

```bash
cargo xtask fpga project        # generate the Vivado project (proj_gen.tcl)
cargo xtask fpga build          # synthesis -> implementation -> bitstream -> autd3-fpga.mcs
cargo xtask fpga build --force  # re-run synthesis even if a bitstream exists
cargo xtask fpga flash          # build, then write autd3-fpga.mcs to the SPI flash
cargo xtask fpga clean
```

`build` generates the project first if it is missing, and reuses
`autd3-fpga.runs/impl_alter_def/top.bit` when it already exists (only `write_cfgmem` runs).


# Author

Shun Suzuki, 2022-2026

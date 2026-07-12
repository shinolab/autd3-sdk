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

## Simulation

```bash
cargo xtask fpga sim                 # run every testbench in rtl/sim_1/ and report pass/fail
cargo xtask fpga sim --tb sim_mem_cnt  # run a single testbench (module name)
```

`sim` generates the Vivado project first if it is missing, then drives `xsim`
(`sim.tcl`) over the testbenches (module name == file name, `sim_helper_*` excluded).
A testbench passes when its simulation reaches `$finish` with no `ASSERT_EQ` failure;
a `SUMMARY` table is printed and the command exits non-zero if any testbench fails.
Vivado (`xsim`) is required, so `sim` is a local-only task and is not part of CI.


# Author

Shun Suzuki, 2022-2026

# autd3-console

GUI console that launches the AUTD3 tools: the sound field simulator and the CPU/FPGA firmware writer.

## Install

Open the newest `console-v*` entry on the [releases page](https://github.com/shinolab/autd3-sdk/releases) and run the install command shown there.

## Updating

Run the bundled `console-update` command from a terminal, or run the install command of the newer `console-v*` release again.

## Building from source

```console
cargo xtask console run       # build and run
cargo xtask console stage     # build every distributed binary into console/target/distrib
```

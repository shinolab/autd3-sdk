# autd3-console

GUI console that launches the AUTD3 tools: the sound field simulator and the CPU/FPGA firmware writer.

## Install

Open the newest `console-v*` entry on the [releases page](https://github.com/shinolab/autd3-sdk/releases) and run the install command shown there.

## Building from source

```console
cargo xtask console run       # build and run
cargo xtask console stage     # build every distributed binary into console/target/distrib
```

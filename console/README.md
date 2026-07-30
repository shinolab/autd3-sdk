# autd3-console

GUI console that launches the AUTD3 tools: the sound field simulator, the CPU/FPGA firmware writer, and (on Windows) the TwinCAT setup CLI.

## Install

Open the newest `console-v*` entry on the [releases page](https://github.com/shinolab/autd3-sdk/releases) and run the install command shown there.

## Updating

When installed through the installer, autd3-console checks for a newer release on startup and can update itself from the banner at the top of the window.
The automatic check can be turned off in the "About" tab.
The bundled `console-update` command does the same thing from a terminal.

## Building from source

```console
cargo xtask console run       # build and run
cargo xtask console stage     # build every distributed binary into console/target/distrib
cargo xtask console bundle    # stage + archive
```

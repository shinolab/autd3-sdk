# autd3-console

GUI console that launches the AUTD3 tools: the sound field simulator, the CPU/FPGA firmware writer, and (on Windows) the TwinCAT setup CLI.

## Install

Open the newest `console-v*` entry on the [releases page](https://github.com/shinolab/autd3-sdk/releases) and run the install command shown there, or fill the version into the commands below.

Linux / macOS:

```console
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/shinolab/autd3-sdk/releases/download/console-v<version>/console-installer.sh | sh
```

Windows (PowerShell):

```console
irm https://github.com/shinolab/autd3-sdk/releases/download/console-v<version>/console-installer.ps1 | iex
```

The installer puts `autd3-console`, `autd3-rs-simulator`, `autd3-firmware`, and `console-update` into `~/.autd3/bin`.
Prebuilt archives (`console-<target>.tar.xz` / `.zip`) are attached to the same release.

## Updating

When installed through the installer, autd3-console checks for a newer release on startup and can update itself from the banner at the top of the window.
The automatic check can be turned off in the **About** tab.
The bundled `console-update` command does the same thing from a terminal.

## TwinCAT

The TwinCAT setup CLI needs TwinCAT XAE and cannot be built in CI, so it is not part of the installer.
It ships in the manual Windows bundle produced by `cargo xtask console bundle`, and autd3-console looks for it at `twincat/twincat-cli.exe` next to its own executable.

## Building from source

```console
cargo xtask console run       # build and run
cargo xtask console stage     # build every distributed binary into console/target/distrib
cargo xtask console bundle    # stage + archive (adds twincat on Windows)
```

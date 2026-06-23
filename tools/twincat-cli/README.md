# twincat-cli

This program is used to set up TwinCAT for AUTD3.

It drives the TwinCAT XAE Shell through the DTE COM API, so it is **Windows only**.
It is built for .NET Framework 4.8 to reduce binary size (the dependencies do not support Native AOT and trimming).

# Prerequisites

- **TwinCAT XAE** (engineering) installed, plus the **TwinCAT XAE integration**.
- TwinCAT runtime installed (the ESI directory `…\TwinCAT\3.1\Config\Io\EtherCAT\` must exist).

# Build & Run

From `autd3-sdk/`, drive everything via xtask.

```
cargo xtask tool twincat run -- [options]   # scan + set up a TwinCAT project
cargo xtask tool twincat open -- [options]  # reopen the saved project
cargo xtask tool twincat doctor             # diagnose virtualization-based security
cargo xtask tool twincat install-esi        # install the bundled AUTD.xml only
```

Writing under `Program Files (x86)` needs an **Administrator** terminal.

# Usage

```
twincat-cli run [options]

Options:
  -?, -h, --help            Show help and usage information
  -c, --client <IP_ADDR>    Client IP address. If empty, use localhost. []
  --device_name <DEV_NAME>  Ethernet device name. If empty, use the first device found. []
  -s, --sync0 <CYCLE_TIME>  Sync0 cycle time in units of 500μs. [default: 2]
  -t, --task <CYCLE_TIME>   Task cycle time in units of CPU base time. [default: 1]
  -b, --base <TIME>         CPU base time. [default: 1ms]
  --twincat <4024|4026>     TwinCAT version [default: 4026]
  -k, --keep                Keep TwinCAT XAE Shell window open. [default: False]
  --delay <DELAY_MS>        Delay time to wait for the operation to complete (ms). [default: 1000]
  --twincat-root <DIR>      TwinCAT 3.1 install directory (the folder %TwinCAT3Dir% points to). Auto-detected if empty.
  --progid <PROGID>         Override the DTE COM ProgID (e.g. VisualStudio.DTE.17.0). Defaults by --twincat version.
  -d, --debug               Enable debug mode. [default: False]

twincat-cli open [options]

Options:
  --twincat <4024|4026>     TwinCAT version [default: 4026]
  --twincat-root <DIR>      TwinCAT 3.1 install directory (the folder %TwinCAT3Dir% points to). Auto-detected if empty.
  --progid <PROGID>         Override the DTE COM ProgID (e.g. VisualStudio.DTE.17.0). Defaults by --twincat version.
  -d, --debug               Enable debug mode. [default: False]
```

# Author

Shun Suzuki, 2023-2026

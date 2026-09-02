---
title: Setup
description: Preparing the hardware and firmware to start using AUTD3
sidebar:
  order: 1
---

The steps for getting started with AUTD3 are described in the following order.

- [Hardware](/autd3-sdk/en/getting-started/setup/hardware/): Connect the AUTD3 device to the PC.
- [Firmware](/autd3-sdk/en/getting-started/setup/firmware/): Update the device firmware to a compatible version.
- [Software](/autd3-sdk/en/getting-started/setup/software/): Add the SDK library as a dependency.

## Choosing how to drive the bus

AUTD3 is driven over EtherCAT. There are three options, depending on where the EtherCAT master runs.

- [Appliance](/autd3-sdk/en/getting-started/appliance/) (recommended)
  - A dedicated board runs the master and the host connects over TCP. For any host OS, and when you want the bus decoupled from the host's load and OS settings.
- [echocat](/autd3-sdk/en/api/link/echocat/)
  - The host itself becomes the master. When you do not want to add hardware. Needs npcap on Windows and raw socket privileges on Linux/macOS. It may also become unstable under host load.
- [TwinCAT](/autd3-sdk/en/getting-started/twincat/)
  - Windows only. Needs a supported network controller and a TwinCAT setup. The most stable of the three.

Unless there is a reason not to, the [Appliance](/autd3-sdk/en/getting-started/appliance/) is recommended, though it additionally requires a Raspberry Pi 4, a microSD card and a USB Ethernet adapter.

:::note
The tutorial that follows uses `echocat`, to avoid the extra hardware setup.
:::

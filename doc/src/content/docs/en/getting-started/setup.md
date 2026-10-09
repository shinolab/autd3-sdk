---
title: Setup
description: Preparing the hardware and firmware to start using AUTD3
sidebar:
  order: 1
---

The steps for getting started with AUTD3 are described in the following order.

- [Hardware](/autd3-sdk/en/getting-started/setup/hardware/): Connect the AUTD3 device to the PC.
- [Firmware](/autd3-sdk/en/getting-started/setup/firmware/): Update the device firmware to a compatible version.
- [Host Network Setup](/autd3-sdk/en/getting-started/setup/network/): Configure the host firewall and NIC.
- [Software](/autd3-sdk/en/getting-started/setup/software/): Add the SDK library as a dependency.

If your devices run firmware v0.9.x or earlier, first follow [Migrating from firmware v0.9.x](/autd3-sdk/en/getting-started/migration/).

## Choosing how to connect

AUTD3 and the host communicate over [UDP](/autd3-sdk/en/api/connection/) on IPv6 link-local addresses.
The host NIC connects to the AUTD3 directly. No extra hardware is required. The host needs neither an IP address configuration nor root / administrator privileges; only the [host network setup](/autd3-sdk/en/getting-started/setup/network/) is needed.

:::note
Without hardware, the [device emulator](/autd3-sdk/en/api/connection/device-emulator/) and the [Simulator](/autd3-sdk/en/guide/simulator/) are opened the same way.
:::

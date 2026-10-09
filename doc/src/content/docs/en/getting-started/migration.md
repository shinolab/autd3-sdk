---
title: Migrating from firmware v0.9.x
description: Moving devices on the EtherCAT firmware (v0.9.x and earlier) to the UDP firmware (v0.10.x)
sidebar:
  order: 4
---

Devices on firmware v0.9.x and earlier communicate with the host over EtherCAT.
Firmware v0.10.0 and later communicates with the host over UDP, and this SDK supports only the UDP firmware.
This page describes how to move a v0.9.x device to v0.10.x.

:::note
To keep using the EtherCAT firmware, stay on SDK v0.9.
The libraries and tools of this SDK cannot connect to devices on the EtherCAT firmware.
:::

## Migrating via JTAG

Devices on the EtherCAT firmware cannot be updated over OTA (the OTA of this SDK reaches only devices on the UDP firmware).
Write v0.10.x to both the CPU and the FPGA (`Both`) from autd3-console of this SDK with the JTAG procedure in [Firmware](/autd3-sdk/en/getting-started/setup/firmware/#updating-via-jtag).
Once the devices run the UDP firmware, later updates can be done over OTA.

## Going Back to the EtherCAT Firmware

The OTA cannot take a device back to the EtherCAT firmware.
To go back to the EtherCAT firmware, write it via JTAG from autd3-console of the old SDK (v0.9).

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
The libraries of this SDK cannot connect to devices on the EtherCAT firmware. EtherCAT is used only by the migration OTA below.
:::

## Migrating over OTA

When the OTA of autd3-console in this SDK finds no device over UDP, it looks for devices over EtherCAT.
If it finds v0.9.x devices over EtherCAT, it writes the UDP CPU firmware over EtherCAT, reconnects to the rebooted devices over UDP, and confirms the image.
The v0.9.x firmware does not check the kind of image it receives over OTA, so this moves the devices to the UDP firmware.

### Prerequisites

- Complete the [host network setup](/autd3-sdk/en/getting-started/setup/network/) (the rebooted devices are reached over UDP)
- Prepare the host to reach the devices over EtherCAT
  - Windows: install [Npcap](https://npcap.com/) with "WinPcap API-compatible Mode" enabled
  - Linux: grant `autd3-rs-firmware-ota` bundled with autd3-console the raw socket capability (`sudo setcap cap_net_raw,cap_sys_nice+ep autd3-rs-firmware-ota`)
  - macOS: grant read/write access to `/dev/bpf*`

### Procedure

1. Connect the AUTD3 and the PC with an Ethernet cable, then turn on the power
1. Launch autd3-console and open the **Firmware** tab
1. Choose v0.10.x as **Version** and `Both` as **Target**
1. Make sure **Method** is OTA (the default), then press **Flash**
1. The log shows `connected over EtherCAT` and the CPU update starts. After it, the devices reboot, and the tool reconnects over UDP to confirm the CPU firmware. The FPGA is then written over UDP
1. The migration is complete when `Ok!` appears in the log

The CPU firmware stays on trial until the tool reconnects over UDP after the reboot and confirms it.
If the reconnection over UDP fails (a missing network setup, a firewall, ...), fix the cause **without powering off the devices** and run the same procedure again to confirm it.
If the devices are powered off before it is confirmed, they boot the original EtherCAT firmware. In that case, start over.

When devices on the EtherCAT firmware and on the UDP firmware are mixed in one chain, neither transport reaches all of them.
If only some of the devices were moved, connect each remaining device directly to the PC and repeat the procedure.

## Migrating via JTAG

If the OTA cannot be used, write v0.10.x to both the CPU and the FPGA (`Both`) from autd3-console of this SDK with the JTAG procedure in [Firmware](/autd3-sdk/en/getting-started/setup/firmware/#updating-via-jtag).

## Going Back to the EtherCAT Firmware

The UDP firmware refuses EtherCAT images over OTA.
To go back to the EtherCAT firmware, write it via JTAG from autd3-console of the old SDK (v0.9).

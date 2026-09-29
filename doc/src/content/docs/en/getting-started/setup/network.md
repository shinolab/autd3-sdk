---
title: Host Network Setup
description: Firewall and NIC settings for talking to AUTD3 over UDP
sidebar:
  order: 3
---

The host and AUTD3 communicate over UDP on IPv6 link-local addresses.
The UDP port on the device side is **44336**.

No IP address needs to be configured on the host NIC, and no root / administrator privileges are required.
The following settings are needed, however.

## Enable IPv6

IPv6 must be enabled on the NIC the AUTD3 is attached to.
A NIC with IPv6 disabled gets no link-local address and cannot communicate.

- Windows: make sure "Internet Protocol Version 6 (TCP/IPv6)" is checked in the adapter properties
- Linux: make sure `ip -6 addr show dev <if>` shows an address starting with `fe80::`

## Allow Inbound Traffic in the Firewall

The replies of the devices come from a different address than the (multicast) destination the host sent to.
A stateful firewall therefore treats the replies as new inbound traffic and drops them.
Allow inbound traffic from UDP source port 44336 on the NIC the AUTD3 is attached to.

### Linux

With `ufw`, allow it as follows.

```sh
sudo ufw allow in on <if> proto udp from fe80::/10 port 44336
```

Replace `<if>` with the name of the NIC the AUTD3 is attached to (e.g. `enp3s0`).
With `firewalld` or `nftables`, allow inbound traffic with the same conditions (interface, source `fe80::/10`, UDP source port 44336).

### Windows

Add an inbound rule in "Windows Defender Firewall with Advanced Security".

1. Choose **Inbound Rules** → **New Rule** → **Custom**
1. Choose **UDP** as the protocol type and set **Remote port** to `44336`
1. Choose **Allow the connection** and apply it to the network profile of the NIC the AUTD3 is attached to

From PowerShell (as administrator) it can be added as follows.

```powershell
New-NetFirewallRule -DisplayName "AUTD3" -Direction Inbound -Protocol UDP -RemotePort 44336 -Action Allow
```

### macOS

If the application firewall is enabled, allow inbound connections for the application that uses AUTD3 (when using it from Python or `dotnet`, for those executables).

## Disable Interrupt Moderation

Interrupt moderation (interrupt coalescing) on the NIC increases and spreads the receive latency.
It can delay a reply by hundreds of µs or more and cause frames to be sent again, so disabling it is recommended.

- Linux: `sudo ethtool -C <if> rx-usecs 0`
- Windows: open the adapter properties in Device Manager and set "Interrupt Moderation" under **Advanced** to "Disabled"

## About L2 Switches

Connecting the host directly to the first AUTD3 is recommended.
With an L2 switch (hub) in between, a switch that has MLD snooping enabled may not forward the neighbor discovery multicast, and communication can fail.
In that case, disable MLD snooping on the switch.

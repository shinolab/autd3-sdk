---
title: Hardware
description: Steps to connect the AUTD3 device to the PC
sidebar:
  order: 1
slug: 0.5.x/en/0.3.x/getting-started/setup/hardware
---

This section describes the steps to connect the AUTD3 device to the PC.
For details on the device configuration, dimensions, coordinate system, and connectors, see [Guide/AUTD3](/autd3-sdk/en/0.5.x/0.3.x/hardware/board/).

## EtherCAT Connection

Connect the PC and the EtherCAT In of the first AUTD3 with an Ethernet cable.
When using multiple devices, connect the EtherCAT Out of the n-th device to the EtherCAT In of the (n+1)-th device in order (daisy chain).

:::caution
Use an Ethernet cable of CAT 5e or higher.
:::

## Power Supply

The AUTD3 power supply uses a 24 V DC power source.
For wiring details, see [Guide/AUTD3](/autd3-sdk/en/0.5.x/0.3.x/hardware/board/#power-supply).

The AUTD3 device itself has no power switch or similar, and operation begins the moment power is supplied.

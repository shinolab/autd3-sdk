---
title: Hardware
description: Steps to connect the AUTD3 device to the PC
sidebar:
  order: 1
slug: en/0.10.x/getting-started/setup/hardware
---

This section describes the steps to connect the AUTD3 device to the PC.
For details on the device configuration, dimensions, coordinate system, and connectors, see [Guide/AUTD3](/autd3-sdk/en/0.10.x/hardware/board/).

## Ethernet Connection

Connect the PC and the middle one of the three Ethernet ports of the first AUTD3 (labeled CN10 on the board) with an Ethernet cable.
When using multiple devices, connect the outermost port (CN11) of the n-th device to the middle port (CN10) of the (n+1)-th device.

The devices are numbered 0, 1, 2, ... from the one nearest to the PC.

:::caution
Use an Ethernet cable of CAT 5e or higher.
:::

## Power Supply

The AUTD3 power supply uses a 24 V DC power source.
For wiring details, see [Guide/AUTD3](/autd3-sdk/en/0.10.x/hardware/board/#power-supply).

The AUTD3 device itself has no power switch or similar, and operation begins the moment power is supplied.

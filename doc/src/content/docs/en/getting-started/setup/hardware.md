---
title: Hardware
description: Steps to connect the AUTD3 device to the PC
sidebar:
  order: 1
---

This section describes the steps to connect the AUTD3 device to the PC.
For details on the device configuration, dimensions, coordinate system, and connectors, see [Guide/AUTD3](/autd3-sdk/en/hardware/board/).

## Ethernet Connection

Connect the PC and the Ethernet port of the first AUTD3 (labeled EtherCAT In on the board) with an Ethernet cable.
When using multiple devices, connect the EtherCAT Out of the n-th device to the EtherCAT In of the (n+1)-th device in order (daisy chain).

The devices are numbered 0, 1, 2, ... from the one nearest to the PC.
Either port of a device may face the PC, but keeping In on the PC side as above makes the wiring easier to follow.

Connecting the PC directly to the first device, without an L2 switch (hub) in between, is recommended.

:::caution
Use an Ethernet cable of CAT 5e or higher.
:::

## Power Supply

The AUTD3 power supply uses a 24 V DC power source.
For wiring details, see [Guide/AUTD3](/autd3-sdk/en/hardware/board/#power-supply).

The AUTD3 device itself has no power switch or similar, and operation begins the moment power is supplied.

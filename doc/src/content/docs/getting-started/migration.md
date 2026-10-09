---
title: ファームウェア v0.9.x からの移行
description: EtherCAT 版ファームウェア (v0.9.x 以前) のデバイスを UDP 版 (v0.10.x) へ移行する手順
sidebar:
  order: 4
---

ファームウェア v0.9.x 以前のデバイスは, ホストと EtherCAT で通信する.
v0.10.0 以降のファームウェアはホストと UDP で通信し, この SDK は UDP 版のファームウェアだけをサポートする.
ここでは, v0.9.x のデバイスを v0.10.x へ移行する手順を説明する.

:::note
EtherCAT 版のファームウェアを使い続ける場合は, SDK v0.9 系を使い続けること.
この SDK のライブラリとツールから EtherCAT 版のデバイスには接続できない.
:::

## JTAG で移行する

EtherCAT 版のデバイスは OTA では更新できない (この SDK の OTA は UDP 版のデバイスにしか届かない).
[ファームウェア](/autd3-sdk/getting-started/setup/firmware/#jtag-を使用したアップデート) の JTAG の手順で, この SDK の autd3-console から CPU と FPGA の両方 (`Both`) に v0.10.x を書き込む.
一度 UDP 版にすれば, 以降の更新は OTA で行える.

## EtherCAT 版に戻す

OTA では EtherCAT 版のファームウェアに戻せない.
EtherCAT 版に戻す場合は, 旧 SDK (v0.9 系) の autd3-console から JTAG で書き込む.

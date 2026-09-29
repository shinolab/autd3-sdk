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
この SDK のライブラリから EtherCAT 版のデバイスには接続できない. EtherCAT を扱うのは, 下記の移行用の OTA だけである.
:::

## OTA で移行する

この SDK の autd3-console の OTA は, UDP でデバイスが見つからないと EtherCAT でデバイスを探す.
EtherCAT で v0.9.x のデバイスが見つかると, EtherCAT 経由で UDP 版の CPU ファームウェアを書き込み, 再起動したデバイスに UDP で接続し直して確定する.
v0.9.x のファームウェアは OTA で受け取るイメージの種類を検査しないので, この手順で UDP 版に移行できる.

### 事前準備

- [ホストのネットワーク設定](/autd3-sdk/getting-started/setup/network/) を済ませておくこと (再起動後のデバイスには UDP で接続する)
- EtherCAT でデバイスに接続するための準備をする
  - Windows: [Npcap](https://npcap.com/) を "WinPcap API-compatible Mode" を有効にしてインストールする
  - Linux: autd3-console に同梱の `autd3-rs-firmware-ota` に raw socket の権限を与える (`sudo setcap cap_net_raw,cap_sys_nice+ep autd3-rs-firmware-ota`)
  - macOS: `/dev/bpf*` の読み書き権限を与える

### 手順

1. AUTD3 と PC を Ethernet ケーブルで接続し, 電源を入れる
1. autd3-console を起動し, **Firmware** タブを開く
1. **Version** に v0.10.x, **Target** に `Both` を選ぶ
1. **Method** が OTA (既定) になっていることを確認し, **Flash** を押す
1. ログに `connected over EtherCAT` と出て, CPU の書き込みが始まる. 書き込みの後デバイスは再起動し, UDP で接続し直して CPU のファームウェアを確定する. 続けて FPGA が UDP で書き込まれる
1. ログに `Ok!` が出れば移行は完了

CPU のファームウェアは, 再起動後に UDP で接続し直して確定するまでは試行扱いである.
UDP での再接続に失敗した場合 (ネットワーク設定の漏れ, ファイアウォールなど) は, **電源を切らずに**原因を取り除いてから, 同じ手順をもう一度行えば確定する.
確定する前に電源を切ると, デバイスは元の EtherCAT 版のファームウェアで起動する. その場合は最初からやり直す.

EtherCAT 版と UDP 版のデバイスが 1 本の鎖に混ざると, どちらの方法でも全台には届かない.
一部のデバイスだけが移行した状態で止まった場合は, 残ったデバイスを 1 台ずつ PC に直結して同じ手順を行う.

## JTAG で移行する

OTA が使えない場合は, [ファームウェア](/autd3-sdk/getting-started/setup/firmware/#jtag-を使用したアップデート) の JTAG の手順で, この SDK の autd3-console から CPU と FPGA の両方 (`Both`) に v0.10.x を書き込む.

## EtherCAT 版に戻す

UDP 版のファームウェアは EtherCAT 版のイメージを OTA で受け付けない.
EtherCAT 版に戻す場合は, 旧 SDK (v0.9 系) の autd3-console から JTAG で書き込む.

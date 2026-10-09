---
title: セットアップ
description: AUTD3 を使い始めるためのハードウェア・ファームウェアの準備
sidebar:
  order: 1
---

AUTD3 を使い始めるための準備を, 以下の順に説明する.

- [ハードウェア](/autd3-sdk/getting-started/setup/hardware/): AUTD3 デバイスと PC を接続する.
- [ファームウェア](/autd3-sdk/getting-started/setup/firmware/): デバイスのファームウェアを対応バージョンへ更新する.
- [ホストのネットワーク設定](/autd3-sdk/getting-started/setup/network/): ホストのファイアウォールと NIC を設定する.
- [ソフトウェア](/autd3-sdk/getting-started/setup/software/): SDK ライブラリを依存に追加する.

ファームウェア v0.9.x 以前のデバイスを使っている場合は, 先に [ファームウェア v0.9.x からの移行](/autd3-sdk/getting-started/migration/) を行うこと.

## 接続方法を選ぶ

AUTD3 とホストは [UDP](/autd3-sdk/api/connection/)/IPv6 リンクローカルで通信する.
ホストの NIC から AUTD3 に直接つなぐ. 追加のハードウェアは要らない. ホストに IP アドレスの設定や root / 管理者権限は不要で, [ホストのネットワーク設定](/autd3-sdk/getting-started/setup/network/) だけを行えばよい.

:::note
実機が無い場合は, [デバイスエミュレータ](/autd3-sdk/api/connection/device-emulator/) や [Simulator](/autd3-sdk/guide/simulator/) に同じ方法でつなげる.
:::

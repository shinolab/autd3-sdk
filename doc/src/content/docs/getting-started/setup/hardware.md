---
title: ハードウェア
description: AUTD3 デバイスと PC の接続手順
sidebar:
  order: 1
---

ここでは, AUTD3 デバイスと PC を接続する手順を説明する.
デバイスの構成・寸法・座標系・コネクタなどの詳細は [ガイド/AUTD3](/autd3-sdk/hardware/board/) を参照すること.

## EtherCAT の接続

PC と 1 台目の AUTD3 の EtherCAT In を Ethernet ケーブルで接続する.
複数台を使う場合は, n 台目の EtherCAT Out と n+1 台目の EtherCAT In を順に接続する (デイジーチェーン).

:::caution
Ethernet ケーブルは CAT 5e 以上のものを使用すること.
:::

## 電源

AUTD3 の電源は 24 V の直流電源を使用する.
配線の詳細は [ガイド/AUTD3](/autd3-sdk/hardware/board/#電源) を参照すること.

AUTD3 デバイス本体には電源スイッチ等はなく, 電源を供給した時点から動作が始まる.


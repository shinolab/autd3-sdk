---
title: ハードウェア
description: AUTD3 デバイスと PC の接続手順
sidebar:
  order: 1
---

ここでは, AUTD3 デバイスと PC を接続する手順を説明する.
デバイスの構成・寸法・座標系・コネクタなどの詳細は [ガイド/AUTD3](/autd3-sdk/hardware/board/) を参照すること.

## Ethernet の接続

PC と 1 台目の AUTD3 の Ethernet ポート (基板上の表記は EtherCAT In) を Ethernet ケーブルで接続する.
複数台を使う場合は, n 台目の EtherCAT Out と n+1 台目の EtherCAT In を順に接続する (デイジーチェーン).

PC に近い機体から順に 0, 1, 2, ... 番のデバイスになる.
デバイスのどちらのポートを PC 側にしてもよいが, 上記のように In を PC 側にそろえておくと配線を追いやすい.

PC と 1 台目の間に L2 スイッチ (ハブ) を挟まず, 直結することを推奨する.

:::caution
Ethernet ケーブルは CAT 5e 以上のものを使用すること.
:::

## 電源

AUTD3 の電源は 24 V の直流電源を使用する.
配線の詳細は [ガイド/AUTD3](/autd3-sdk/hardware/board/#電源) を参照すること.

AUTD3 デバイス本体には電源スイッチ等はなく, 電源を供給した時点から動作が始まる.


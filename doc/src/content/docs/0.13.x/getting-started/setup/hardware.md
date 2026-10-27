---
title: ハードウェア
description: AUTD3 デバイスと PC の接続手順
sidebar:
  order: 1
slug: 0.13.x/getting-started/setup/hardware
---

ここでは, AUTD3 デバイスと PC を接続する手順を説明する.
デバイスの構成・寸法・座標系・コネクタなどの詳細は [ガイド/AUTD3](/autd3-sdk/0.13.x/hardware/board/) を参照すること.

## Ethernet の接続

PC と 1 台目の AUTD3 の 3つある Ethernet ポートの真ん中のもの (基板上の表記は CN10) を Ethernet ケーブルで接続する.
複数台を使う場合は, n 台目のデバイスの最も外側のポート (CN11) と n+1 台目のデバイスの真ん中のポート (CN10) を接続する.

PC に近い機体から順に 0, 1, 2, ... 番目のデバイスになる.

:::caution
Ethernet ケーブルは CAT 5e 以上のものを使用すること.
:::

## 電源

AUTD3 の電源は 24 V の直流電源を使用する.
配線の詳細は [ガイド/AUTD3](/autd3-sdk/0.13.x/hardware/board/#電源) を参照すること.

AUTD3 デバイス本体には電源スイッチ等はなく, 電源を供給した時点から動作が始まる.

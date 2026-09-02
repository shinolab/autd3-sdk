---
title: セットアップ
description: AUTD3 を使い始めるためのハードウェア・ファームウェアの準備
sidebar:
  order: 1
---

AUTD3 を使い始めるための準備を, 以下の順に説明する.

- [ハードウェア](/autd3-sdk/getting-started/setup/hardware/): AUTD3 デバイスと PC を接続する.
- [ファームウェア](/autd3-sdk/getting-started/setup/firmware/): デバイスのファームウェアを対応バージョンへ更新する.
- [ソフトウェア](/autd3-sdk/getting-started/setup/software/): SDK ライブラリを依存に追加する.

## バスの駆動方法を選ぶ

AUTD3 は EtherCAT で駆動する. EtherCAT マスタをどこで動かすかで, 以下の 3 つの選択肢がある.

- [Appliance](/autd3-sdk/getting-started/appliance/) (推奨) 
  - 専用ボードがマスタを動かし, ホストは TCP でつなぐ. ホストの OS を問わず, ホストの負荷や OS 設定からバスを切り離したい場合.
- [echocat](/autd3-sdk/api/link/echocat/)
  - ホスト自身がマスタになる. 追加のハードウェアを増やしたくない場合. Windows では npcap, Linux/macOS では raw socket の権限が要る. また, ホストの負荷により不安定になる可能性がある.
- [TwinCAT](/autd3-sdk/getting-started/twincat/)
  - Windows限定. 対応するネットワークコントローラと TwinCAT のセットアップが必要. 安定性はもっとも高い.

特に理由がなければ [Appliance](/autd3-sdk/getting-started/appliance/) を推奨するが, Raspberry Pi 4 と microSD カード, USB イーサネットアダプタが追加で必要になる.

:::note
後述のチュートリアルでは, 追加のハードウェアセットアップを避けるため, `echocat` を使用する.
:::

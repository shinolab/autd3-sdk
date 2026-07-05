---
title: ソフトウェア
description: SDK を構成するクレート一覧
sidebar:
  order: 3
slug: 0.1.x/getting-started/setup/software
---

AUTD3 SDK は, 以下の Rust クレート群として提供される.

:::note
Python / C# 向けのバインディングも提供される. 使い方は各 API ページのコードタブを参照.
:::

* [autd3-rs](https://crates.io/crates/autd3-rs): クライアント本体
* [autd3-rs-core](https://crates.io/crates/autd3-rs-core): 基盤層
* [autd3-rs-pattern](https://crates.io/crates/autd3-rs-pattern): パターン計算 (単一焦点等)
* [autd3-rs-pattern-holo](https://crates.io/crates/autd3-rs-pattern-holo): 多焦点最適化
* [autd3-rs-modulation](https://crates.io/crates/autd3-rs-modulation): AM 変調計算
* [autd3-rs-link-ethercrab](https://crates.io/crates/autd3-rs-link-ethercrab): EtherCrab ベースの Link
* [autd3-rs-link-soem](https://crates.io/crates/autd3-rs-link-soem): SOEM ベースの Link ([GPL-3.0-only](/autd3-sdk/0.1.x/misc/license/))
* [autd3-rs-link-remote](https://crates.io/crates/autd3-rs-link-remote): リモート接続用の Link
* [autd3-rs-link-twincat](https://crates.io/crates/autd3-rs-link-twincat): TwinCAT ベースの Link

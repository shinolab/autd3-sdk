---
title: ソフトウェア
description: SDK を構成するクレート一覧
sidebar:
  order: 3
---

AUTD3 SDK は, 以下の Rust クレート群として提供される.

:::note
Python / C# などのバインディングは今後追加予定.
:::

- [autd3-rs](https://crates.io/crates/autd3-rs): クライアント本体.
- [autd3-rs-core](https://crates.io/crates/autd3-rs-core): 基盤層 (`autd3-rs` から再エクスポートされる).
- [autd3-rs-pattern](https://crates.io/crates/autd3-rs-pattern): パターン計算 (焦点合成等).
- [autd3-rs-pattern-holo](https://crates.io/crates/autd3-rs-pattern-holo): 多焦点 (ホログラム) 最適化.
- [autd3-rs-modulation](https://crates.io/crates/autd3-rs-modulation): AM 変調計算.
- [autd3-rs-link-ethercrab](https://crates.io/crates/autd3-rs-link-ethercrab): EtherCrab ベースの Link.
- [autd3-rs-link-soem](https://crates.io/crates/autd3-rs-link-soem): SOEM ベースの Link ([GPL-3.0-only](/autd3-sdk/misc/license/)).
- [autd3-rs-link-remote](https://crates.io/crates/autd3-rs-link-remote): リモート接続用の Link.
- [autd3-rs-link-twincat](https://crates.io/crates/autd3-rs-link-twincat): TwinCAT ベースの Link.
- [autd3-rs-link-nop](https://crates.io/crates/autd3-rs-link-nop): 実機不要の Nop (no-op) Link.
- [autd3-rs-firmware-emulator](https://crates.io/crates/autd3-rs-firmware-emulator): ファームウェアエミュレータ.

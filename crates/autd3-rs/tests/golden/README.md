# legacy_v38_pack.tsv

`autd3_rs::legacy` が組む送信フレームを, 旧 SDK (autd3 v38.1.0) が同条件で組むフレームと
ビット単位で突き合わせるための golden データ.

## 形式

タブ区切り 4 列. 1 行が 1 デバイス分の 1 フレーム (626 byte).

| 列 | 内容 |
|----|------|
| 1  | ケース名 (`crates/autd3-rs/src/legacy/golden.rs` の `assert_case` と 1:1) |
| 2  | ラウンド番号 (0 始まり. 複数フレームに分割される op は 1 op = 複数ラウンド) |
| 3  | デバイス番号 (0 始まり) |
| 4  | フレーム 626 byte の 16 進表現 (小文字, 1252 文字) |

先頭バイト (msg_id) は送信ごとに変わるため, 両側でゼロ化して比較する.

## 生成条件

- 旧 SDK: `autd3` / `autd3-driver` **v38.1.0** (crates.io 版. `legacy/rs/autd3-rs` の
  `v38.1.0` タグと同一内容)
- ジオメトリ: `AUTD3` 2 台. 位置 `(0,0,0)` と `(200,0,0)`, 回転は単位クォータニオン
- 環境: `Environment::new()` (音速 340 m/s)
- パック経路: `Datagram::operation_generator` → `OperationHandler::pack`
  (`parallel = false`). 旧 SDK の `Sender::send` と同じ経路で, リンク送信だけ行わない

## 再生成手順

```console
$ cargo xtask rust golden
```

`generator/` はワークスペース外のスタンドアロンクレート (ルート `Cargo.toml` の `exclude` と
自身の空 `[workspace]` テーブルで隔離) で, 通常のビルド・CI では触られない.
旧 SDK に依存するのはこのクレートだけ.

## ケースを追加するとき

1. `generator/src/main.rs` に `emit(o, g, "<case>", <旧 SDK の Datagram>)` を足す
2. 再生成して `git diff` で **既存ケースの行が 1 byte も変わらないこと** を確認する
3. `crates/autd3-rs/src/legacy/golden.rs` に同名ケースの `assert_case` を足す
4. `golden.rs` の `every_golden_case_is_covered` の `CASES` を更新する
   (これがケース追加のし忘れを検出するガード)

旧 SDK 側で組めない条件 (例: 無限ループ + `SysTime`/`Gpio` の遷移) は golden にできない.
そうしたケースは `crates/autd3-rs/tests/legacy_emulator.rs` 側で検証する.

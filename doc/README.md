# autd3-sdk document

## Build 

```bash
cargo xtask doc samples
cargo xtask doc build   
cargo xtask doc serve    
cargo xtask doc check     
```

## Versioning

`starlight-versions` で現行 docs を「開発版 (next)」とし, リリース版を
`src/content/docs/<slug>/` のスナップショットとして扱う.
コード例は `@codes/...rs?raw` を共有するため, 何もしないとスナップショットも現行コードを表示する.

### 既定: 開発版スナップショット (gitignore + 再生成)

`astro.config.mjs` の `versions` に挙げた slug のスナップショットは, クリーンビルド時に
現行 docs から**再生成**される (`.gitignore` で除外, コミットしない). 常に現行と一致する.
これは「その版がまだ現行開発版そのもの」である間は正しい挙動である
(例: 未リリースの `0.1.x` は今書いている `v0.1.0` と同一なので, 再生成で現行に追従させる).
ローカルで古いスナップショットが残っている場合は `src/content/docs/<slug>/` と
`src/content/versions/` を削除して再ビルドすれば再生成される.

### リリース版の凍結

ある版を**正式にリリースして恒久凍結**する (= 以降 current が次版へ進んでも当時のコードを保つ) 場合:

1. `cargo xtask doc build` で対象 slug のスナップショットを生成する.
2. `cargo xtask doc freeze-version <slug>` を実行する
   (その版の `<CodeTabs rust={excerpt(...)}/>` を解決済みコードでインライン化し, `@codes` 依存を切る).
3. その版ディレクトリを `.gitignore` から外してコミットする.

凍結後の版は `codes/` から独立し, 以降に例ファイルを変更・リネームしても壊れない.
`cargo xtask doc samples` のコンパイル検証対象は現行 `codes/rust` のみ
(凍結済み版のコードは current だった時点で検証済み).


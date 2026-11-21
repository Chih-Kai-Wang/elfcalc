# elfcalc

[English](README.md) · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md) · **日本語** · [한국어](README.ko.md) · [Español](README.es.md)

🎄 **ブラウザで試す: https://chih-kai-wang.github.io/elfcalc/**

**Rust で書かれた高速かつ厳密な Elf Calculator プログラム実行エンジン。ブラウザ上で動くプレイグラウンド付き。**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026) は、短い算術プログラムを作成するコンペティションです。各パズルではレジスタ `r0, r1, …` にいくつかの初期値、複数のターゲット値、そして値の上限が与えられます。プログラムは *rd = ri op rj* を表す命令 `[op, i, j, d]` のリストで、48 個のレジスタと 4 つの演算 `+ - * /` を使います。ターゲットはいずれかのレジスタに現れた瞬間に達成となり、スコアは命令の総数で決まるため、**少ないほど高評価**です。

`elfcalc` はこうしたプログラムを厳密かつ非常に高速にリプレイ実行し、最初に違反したルールを指摘するとともに、結果に寄与しない不要な命令を見つけ出します。

- **プレイグラウンド**：Web ページを開いてプログラムを貼り付けると、レジスタが 1 ステップずつ埋まっていく様子を確認できます。ターゲットは包装されたプレゼントになっており、到達すると開封されます。6 言語に対応：English、繁體中文、简体中文、日本語、한국어、Español。インストールは不要で、Rust コアが WebAssembly（約 80 KB）としてブラウザ内で動作します。
- **ライブラリとコマンドライン**：提出ファイル全体を約 10 ms でチェックし、不要なステップの削除やベンチマーク実行ができます。
- **3 言語ベンチマーク**：同じリプレイ処理を Python、Rust、Goose で実装し、速度差を生んだ要因をまとめています（[BENCHMARKS.md](BENCHMARKS.md)）。

## チェックするルール

| ルール | 違反した場合の扱い |
|---|---|
| セルが `[op, i, j, d]` の JSON 配列であること（`op` は `+ - * /` のいずれか、`i, j, d` は `0..47` の純粋な整数） | `1.0`、`1e0`、`true`、`-0`、`"1"` はすべて却下 |
| プログラムは最大 4,096 命令、セルは最大 131,072 バイト | 却下 |
| 2 つの参照元レジスタの両方に値が入っていること | 開始時に値があるのは初期レジスタのみ |
| `-` は `a > b`、`/` は `b` が `a` をちょうど割り切ること | 0 不可、負数不可、丸め・切り捨て不可 |
| すべての演算結果がパズルの上限以下であること | 中間値も対象 |
| すべてのターゲットがいずれかの時点でいずれかのレジスタに現れること | 初期値もカウント対象 |

乗算の中間結果は 128 ビット整数で計算されるため、オーバーフローによるラップアラウンドは起きません。リリース前に、厳格な Python チェッカー（`json.loads` でパースした後に負のリテラルを却下し、リプレイ実行するもの）と照合して 41,050 件のケースをテストしました。

- 実際のプログラムと、それにランダムな変異を加えたもの
- 手書きのエッジケース：`"\u002b"` などのエスケープ表記、空白文字、巨大な整数、深いネスト、`NaN`

両者の判定が食い違ったケースは 1 件もありませんでした。

## コマンドライン

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # 判定、スコア、不要なステップ数
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # 不要なステップを取り除いた版を書き出す
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check` は重複・未知・欠損・不正な形式の行を報告し、問題があれば終了コード 1 で終了します。速度を重視して作られているため、アップロード前の最終チェックにはバイト単位の CSV 検証ツール [santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier) も併用してください。

## ライブラリ

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // 有効、4 命令
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // 長くなることはなく、常に有効のまま
```

`check` と `check_cell` はまずパズル自体を検証し、パニックすることは決してありません。`strip_dead` は、どのターゲットの初回出現にもつながらない命令を取り除きます。出力は常に有効であり元の長さより増えることはありませんが、必ずしも最小になるとは限りません。すでに達成済みのターゲットを偶然再計算している不要な命令チェーンはそのまま残ります。

## プレイグラウンド

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html` はオフラインでも動作する自己完結型の単一ファイルです。ページから利用する C ABI は `ffi` フィーチャーの後ろに置かれているため、通常の Rust ライブラリはグローバルシンボルを一切エクスポートしません。

## ライセンス

MIT。詳細は [LICENSE](LICENSE) をご覧ください。プレイグラウンドは SIL Open Font License のもとで、Google Fonts からフォント（Fraunces、Plus Jakarta Sans、JetBrains Mono、Noto Sans CJK）を読み込みます。

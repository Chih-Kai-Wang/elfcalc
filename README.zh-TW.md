# elfcalc

[English](README.md) · **繁體中文** · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Español](README.es.md)

🎄 **在瀏覽器裡試玩: https://chih-kai-wang.github.io/elfcalc/**

**以 Rust 撰寫、快速且精確的 Elf Calculator 程式執行引擎，並附帶可在瀏覽器中直接執行的互動演練場。**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026) 要求參賽者寫出簡短的算術程式。每道題目會在暫存器 `r0, r1, …` 中給定幾個初始值、一些目標數字以及一個數值上限。程式是由指令 `[op, i, j, d]` 組成的清單，代表 *rd = ri op rj*，共有 48 個暫存器，支援四種運算 `+ - * /`。只要目標數字出現在任一暫存器中就立刻算達成；總分是所有題目的指令總數，因此**越少越好**。

`elfcalc` 能精確且極快地重播執行這些程式，指出程式違反的第一條規則，並找出完全派不上用場的無用指令。

- **互動演練場**：打開網頁、貼上程式，就能一步一步看著暫存器填滿數值。每個目標都是包好的禮物，達成時就會拆開。支援六種語言：English、繁體中文、简体中文、日本語、한국어、Español。無需安裝任何軟體：Rust 核心會以 WebAssembly（約 80 KB）在網頁中直接執行。
- **函式庫與命令列工具**：約 10 ms 即可檢查整份提交檔案、移除無用步驟，並進行效能基準測試。
- **三語言基準測試**：分別以 Python、Rust 與 Goose 實作同一套重播邏輯，並記錄造成效能差異的關鍵因素（見 [BENCHMARKS.md](BENCHMARKS.md)）。

## 檢查的規則

| 規則 | 違反時的處理方式 |
|---|---|
| 儲存格必須是由 `[op, i, j, d]` 組成的 JSON 陣列，其中 `op` 為 `+ - * /` 之一，`i, j, d` 為 `0..47` 的純整數 | `1.0`、`1e0`、`true`、`-0` 與 `"1"` 一律拒收 |
| 每個程式最多 4,096 條指令，且該儲存格最多 131,072 位元組 | 拒收 |
| 兩個來源暫存器都必須已有數值 | 開始時只有初始暫存器有值 |
| `-` 必須滿足 `a > b`，`/` 必須滿足 `b` 能整除 `a` | 不可為零、不可為負、不可四捨五入或取整 |
| 每個運算結果皆不得超過題目的數值上限 | 中間值也算在內 |
| 每個目標都必須在某個時刻出現在任一暫存器中 | 初始值也算在內 |

中間乘積以 128 位元整數計算，因此絕對不會溢位回繞。在釋出之前，這個解析器已經與嚴格的 Python 驗證程式（先執行 `json.loads`，再拒收負數字面值，接著重播執行）針對 41,050 個測試案例進行比對：

- 真實程式以及對其進行隨機變異後的程式；
- 手寫的邊界案例：例如 `"\u002b"` 等跳脫字元、空白字元、超大整數、深層巢狀結構、`NaN`。

兩者在所有案例中的判定結果完全一致，沒有任何分歧。

## 命令列

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # 判定結果、分數與無用步驟數
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # 輸出移除無用步驟後的版本
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check` 會回報重複、未知、缺漏與格式錯誤的資料列，只要發現任何問題就會以結束碼 1 結束。它是為速度而設計的；上傳前做最後把關時，請一併執行逐位元組檢查的 CSV 驗證器 [santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier)。

## 函式庫

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // 合法，共 4 條指令
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // 絕不會變長，且永遠保持合法
```

`check` 與 `check_cell` 會先驗證題目本身，且永遠不會發生 panic。`strip_dead` 會移除所有對任何目標首次出現沒有貢獻的指令。輸出結果永遠合法，且絕不會比原程式長，但未必保證是絕對最短：如果有一串無用指令剛好重新算出了一個先前已達成的目標，這串指令會被保留下來。

## 互動演練場

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html` 是單一獨立檔案，離線也能執行。網頁使用的 C ABI 放在 `ffi` feature 底下，因此純 Rust 函式庫不會匯出任何全域符號。

## 授權條款

MIT。詳見 [LICENSE](LICENSE)。互動演練場會從 Google Fonts 載入字型（Fraunces、Plus Jakarta Sans、JetBrains Mono、Noto Sans CJK），其授權為 SIL Open Font License。

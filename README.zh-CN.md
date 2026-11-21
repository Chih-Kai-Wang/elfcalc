# elfcalc

[English](README.md) · [繁體中文](README.zh-TW.md) · **简体中文** · [日本語](README.ja.md) · [한국어](README.ko.md) · [Español](README.es.md)

🎄 **在浏览器中试玩: https://chih-kai-wang.github.io/elfcalc/**

**用 Rust 编写、快速且精确的 Elf Calculator 程序执行引擎，附带可直接在浏览器中运行的演练场。**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026) 要求编写简短的算术程序。每道题会在寄存器 `r0, r1, …` 中给出若干初始值、一些目标数字以及一个数值上限。程序是由指令 `[op, i, j, d]` 组成的列表，表示 *rd = ri op rj*，共有 48 个寄存器和 `+ - * /` 四种运算。目标数字一旦出现在任意寄存器中即算达成，总分为全部题目的指令总数，因此**越少越好**。

`elfcalc` 可以精确且极快地重放运行这些程序，指出程序违反的第一条规则，并找出从未派上用场的无用指令。

- **演练场**：打开网页，粘贴一段程序，即可一步步看着寄存器被填满。目标被包装成礼物，达成时就会打开。支持六种语言：English、繁體中文、简体中文、日本語、한국어、Español。无需安装任何软件：Rust 核心以 WebAssembly（约 80 KB）形式直接在页面中运行。
- **库与命令行工具**：约 10 ms 即可校验整份提交文件、剔除无用步骤，并运行性能基准测试。
- **三语言基准测试**：分别用 Python、Rust 和 Goose 实现同一套重放逻辑，并附有关于性能差异原因的说明（见 [BENCHMARKS.md](BENCHMARKS.md)）。

## 检查的规则

| 规则 | 违反时的处理 |
|---|---|
| 单元格必须是由 `[op, i, j, d]` 组成的 JSON 数组，其中 `op` 为 `+ - * /` 之一，`i, j, d` 为 `0..47` 的纯整数 | `1.0`、`1e0`、`true`、`-0` 和 `"1"` 全部拒收 |
| 每个程序最多 4,096 条指令，且单元格最多 131,072 字节 | 拒收 |
| 两个源寄存器中都必须已有值 | 开始时只有初始寄存器有值 |
| `-` 要求 `a > b`，`/` 要求 `b` 能整除 `a` | 不能为零、不能为负、不能舍入或取整 |
| 每个运算结果都不超过题目的数值上限 | 中间值同样计入 |
| 每个目标都在某一时刻出现在某个寄存器中 | 初始值同样计入 |

中间乘积使用 128 位整数计算，因此绝不会发生溢出回绕。在发布之前，解析器已经与严格的 Python 校验器（先调用 `json.loads`，再拒收负数字面量，然后重放运行）在 41,050 个用例上进行了对照测试：

- 真实程序及其随机变异版本；
- 手写的边界用例：例如 `"\u002b"` 等转义写法、空白字符、大整数、深层嵌套、`NaN`。

两者在所有用例中没有任何分歧。

## 命令行

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # 判定结果、得分、无用步骤数
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # 写出移除无用步骤后的版本
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check` 会报告重复、未知、缺失和格式错误的行，遇到任何问题均以退出码 1 退出。它面向极致速度设计；在上传前做最终检查时，建议同时运行逐字节检查的 CSV 校验器 [santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier)。

## 库

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // 合法，共 4 条指令
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // 绝不会更长，且始终合法
```

`check` 和 `check_cell` 会先校验题目本身，且永远不会 panic。`strip_dead` 会移除所有对任何目标首次出现没有贡献的指令。返回结果始终合法且绝不会变长，但不一定是最短的：如果有一条无用指令链恰好重新计算了一个此前已经达成的目标，这条链会被保留下来。

## 演练场

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html` 是独立自包含的单个文件，离线也能正常使用。页面所需的 C ABI 位于 `ffi` feature 背后，因此纯 Rust 库不会导出任何全局符号。

## 许可证

MIT。详见 [LICENSE](LICENSE)。演练场会从 Google Fonts 加载字体（Fraunces、Plus Jakarta Sans、JetBrains Mono、Noto Sans CJK），这些字体采用 SIL Open Font License 授权。

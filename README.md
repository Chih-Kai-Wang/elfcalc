# elfcalc

**English** · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Español](README.es.md)

🎄 **Try it in your browser: https://chih-kai-wang.github.io/elfcalc/**

**A fast, exact engine for Elf Calculator programs, written in Rust, with a playground that runs in your
browser.**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026) asks for short arithmetic
programs. Each puzzle gives a few starting values in registers `r0, r1, …`, some target numbers and a value limit.
A program is a list of instructions `[op, i, j, d]` meaning *rd = ri op rj*, with 48 registers and the operations
`+ - * /`. A target counts as soon as it appears in any register, and the score is the total number of
instructions, so fewer is better.

`elfcalc` replays such programs exactly and very quickly, tells you the first rule a program breaks, and finds
instructions that never help.

- **Playground**: open the web page, paste a program and watch the registers fill step by step. Targets are
  wrapped gifts that open when they are reached. Six languages: English, 繁體中文, 简体中文, 日本語, 한국어,
  Español. Nothing to install: the Rust core runs in the page as WebAssembly (about 80 KB).
- **Library and command line**: check a whole submission in about 10 ms, strip dead steps, and benchmark.
- **A three-language benchmark**: the same replay written in Python, Rust and Goose, with notes on what made the
  difference ([BENCHMARKS.md](BENCHMARKS.md)).

## The rules it checks

| Rule | What happens if it is broken |
|---|---|
| A cell is a JSON array of `[op, i, j, d]`, with `op` one of `+ - * /` and `i, j, d` plain integers 0..47 | `1.0`, `1e0`, `true`, `-0` and `"1"` are all rejected |
| A program has at most 4,096 instructions and its cell is at most 131,072 bytes | rejected |
| Both source registers hold a value | only the starting registers begin filled |
| `-` needs `a > b`, `/` needs `b` to divide `a` exactly | no zero, no negatives, no rounding |
| Every result is at most the puzzle's limit | intermediate values count too |
| Every target appears in some register at some time | starting values count |

Intermediate products are computed in 128-bit integers, so nothing can wrap. Before release, the parser was tested
against a strict Python checker (`json.loads`, then rejecting negative literals, then replaying) on 41,050 cases:

- real programs and random mutations of them;
- hand-written edge cases: escapes such as `"\u002b"`, whitespace, big integers, deep nesting, `NaN`.

The two disagreed on none of them.

## Command line

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # verdict, score, dead steps
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # write a version without dead steps
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check` reports duplicate, unknown, missing and malformed rows, and exits with 1 on any problem. It is built for
speed. For the final check before you upload, also run the byte-by-byte CSV verifier
[santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier).

## Library

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // legal, 4 instructions
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // never longer, always still legal
```

`check` and `check_cell` validate the puzzle first and never panic. `strip_dead` removes instructions that don't
lead to the first appearance of any target. The result is always legal and never longer. It isn't always minimal:
a dead chain that happens to recompute an already-covered target is kept.

## Playground

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html` is one self-contained file that works offline. The C ABI used by the page sits behind the `ffi`
feature, so the plain Rust library exports no global symbols.

## Licence

MIT. See [LICENSE](LICENSE). The playground loads its fonts (Fraunces, Plus Jakarta Sans, JetBrains Mono, Noto
Sans CJK) from Google Fonts, under the SIL Open Font License.

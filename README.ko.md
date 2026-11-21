# elfcalc

[English](README.md) · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · **한국어** · [Español](README.es.md)

🎄 **브라우저에서 바로 사용해 보기: https://chih-kai-wang.github.io/elfcalc/**

**Rust로 작성된 빠르고 정확한 Elf Calculator 프로그램 실행 엔진으로, 브라우저에서 바로 실행되는 플레이그라운드를 함께 제공합니다.**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026)는 짧은 산술 프로그램을 작성하는 대회입니다. 각 퍼즐에는 레지스터 `r0, r1, …`에 담긴 몇 개의 초깃값, 목표 숫자들, 그리고 값 상한이 주어집니다. 프로그램은 *rd = ri op rj*를 뜻하는 명령어 `[op, i, j, d]`의 목록이며, 48개의 레지스터와 `+ - * /` 연산을 사용합니다. 목표 숫자는 어느 레지스터에든 나타나는 즉시 달성으로 인정되며, 점수는 명령어의 총합이므로 **적을수록 좋습니다**.

`elfcalc`는 이런 프로그램을 정확하고 매우 빠르게 리플레이 실행하며, 프로그램이 처음으로 위반한 규칙을 알려주고 전혀 도움이 되지 않는 불필요한 명령어를 찾아냅니다.

- **플레이그라운드**: 웹 페이지를 열고 프로그램을 붙여 넣으면 레지스터가 한 단계씩 채워지는 과정을 볼 수 있습니다. 목표 값은 포장된 선물로 표시되며, 도달하면 열립니다. 6개 언어를 지원합니다: English, 繁體中文, 简体中文, 日本語, 한국어, Español. 별도 설치는 필요 없으며 Rust 코어가 WebAssembly(약 80 KB)로 페이지 안에서 바로 실행됩니다.
- **라이브러리 및 명령줄 도구**: 전체 제출 파일을 약 10 ms 만에 검사하고, 불필요한 단계를 제거하며, 벤치마크를 실행할 수 있습니다.
- **3개 언어 벤치마크**: 동일한 리플레이 로직을 Python, Rust, Goose로 작성하고 성능 차이를 만든 요인을 정리했습니다([BENCHMARKS.md](BENCHMARKS.md)).

## 검사하는 규칙

| 규칙 | 위반 시 처리 |
|---|---|
| 셀이 `[op, i, j, d]`의 JSON 배열이며 `op`는 `+ - * /` 중 하나이고 `i, j, d`는 `0..47` 범위의 순수 정수 | `1.0`, `1e0`, `true`, `-0`, `"1"`은 모두 거부 |
| 프로그램은 최대 4,096개 명령어, 셀은 최대 131,072바이트 | 거부 |
| 두 소스 레지스터 모두 값을 가지고 있어야 함 | 시작할 때는 초기 레지스터만 채워져 있음 |
| `-`는 `a > b`여야 하고, `/`는 `b`가 `a`를 정확히 나누어떨어뜨려야 함 | 0 불가, 음수 불가, 반올림·버림 불가 |
| 모든 결과가 퍼즐의 상한 이하여야 함 | 중간 값도 포함 |
| 모든 목표가 어느 시점에든 어느 레지스터에 나타나야 함 | 초깃값도 포함 |

중간 곱셈은 128비트 정수로 계산되므로 오버플로(래핑)가 발생하지 않습니다. 출시 전에 엄격한 Python 검사기(`json.loads`로 파싱한 뒤 음수 리터럴을 거부하고 리플레이 실행)와 대조해 41,050개 케이스를 테스트했습니다.

- 실제 프로그램과 이를 무작위로 변형한 프로그램
- 직접 작성한 엣지 케이스: `"\u002b"` 같은 이스케이프 표현, 공백 문자, 큰 정수, 깊은 중첩, `NaN`

두 구현의 결과가 불일치한 케이스는 단 한 건도 없었습니다.

## 명령줄

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # 판정, 점수, 불필요한 단계 수
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # 불필요한 단계를 제거한 버전 저장
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check`는 중복되거나 알 수 없거나 누락되었거나 형식이 잘못된 행을 보고하며, 문제가 있으면 종료 코드 1로 종료합니다. 속도에 초점을 맞추어 설계되었으므로, 업로드 전 최종 확인에는 바이트 단위로 검사하는 CSV 검증기 [santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier)도 함께 실행하세요.

## 라이브러리

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // 유효함, 명령어 4개
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // 더 길어지지 않으며 항상 유효함
```

`check`와 `check_cell`은 퍼즐을 먼저 검증하며 절대로 패닉(panic)하지 않습니다. `strip_dead`는 어떤 목표의 최초 등장으로도 이어지지 않는 명령어를 제거합니다. 결과는 항상 유효하며 원래보다 길어지지 않지만, 항상 최소 길이인 것은 아닙니다. 이미 달성된 목표를 우연히 다시 계산하는 불필요한 명령어 체인은 그대로 유지됩니다.

## 플레이그라운드

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html`은 오프라인에서도 동작하는 자체 완결형 단일 파일입니다. 페이지에서 사용하는 C ABI는 `ffi` 기능(feature) 뒤에 분리되어 있어, 일반 Rust 라이브러리는 전역 심볼을 전혀 내보내지 않습니다.

## 라이선스

MIT. 자세한 내용은 [LICENSE](LICENSE)를 참고하세요. 플레이그라운드는 SIL Open Font License에 따라 Google Fonts에서 글꼴(Fraunces, Plus Jakarta Sans, JetBrains Mono, Noto Sans CJK)을 불러옵니다.

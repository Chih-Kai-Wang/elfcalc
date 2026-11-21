# elfcalc

[English](README.md) · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · **Español**

🎄 **Pruébalo en el navegador: https://chih-kai-wang.github.io/elfcalc/**

**Un motor rápido y exacto para programas de Elf Calculator, escrito en Rust, con un entorno interactivo que funciona en tu navegador.**

[Kaggle Santa 2026 – The Elf Calculator](https://www.kaggle.com/competitions/santa-2026) pide programas aritméticos breves. Cada puzle proporciona unos pocos valores iniciales en los registros `r0, r1, …`, algunos números objetivo y un límite de valor. Un programa es una lista de instrucciones `[op, i, j, d]` que significa *rd = ri op rj*, con 48 registros y las operaciones `+ - * /`. Un objetivo cuenta en cuanto aparece en cualquier registro, y la puntuación es el número total de instrucciones, por lo que **cuantas menos, mejor**.

`elfcalc` reproduce esos programas con exactitud y enorme rapidez, te indica la primera regla que incumple un programa y encuentra instrucciones que nunca aportan nada.

- **Entorno interactivo**: abre la página web, pega un programa y observa cómo se llenan los registros paso a paso. Los objetivos son regalos envueltos que se abren al alcanzarlos. Seis idiomas: English, 繁體中文, 简体中文, 日本語, 한국어, Español. No hay nada que instalar: el núcleo en Rust se ejecuta en la página como WebAssembly (unos 80 KB).
- **Biblioteca y línea de comandos**: verifica un envío completo en unos 10 ms, elimina pasos muertos y mide el rendimiento con benchmarks.
- **Una comparativa en tres idiomas**: la misma ejecución escrita en Python, Rust y Goose, con notas sobre qué marcó la diferencia ([BENCHMARKS.md](BENCHMARKS.md)).

## Las reglas que comprueba

| Regla | Qué ocurre si se incumple |
|---|---|
| Una celda es un arreglo JSON de `[op, i, j, d]`, con `op` entre `+ - * /` e `i, j, d` enteros simples en `0..47` | `1.0`, `1e0`, `true`, `-0` y `"1"` son rechazados |
| Un programa tiene como máximo 4,096 instrucciones y su celda ocupa como máximo 131,072 bytes | rechazado |
| Ambos registros fuente contienen un valor | solo los registros iniciales comienzan llenos |
| `-` requiere `a > b`, `/` requiere que `b` divida exactamente a `a` | sin ceros, sin negativos y sin redondeo |
| Todo resultado es como máximo el límite del puzle | los valores intermedios también cuentan |
| Cada objetivo aparece en algún registro en algún momento | los valores iniciales cuentan |

Los productos intermedios se calculan con enteros de 128 bits, de modo que nada puede desbordarse ni dar la vuelta. Antes del lanzamiento, el analizador se probó frente a un verificador estricto en Python (`json.loads`, luego rechazo de literales negativos y luego ejecución) sobre 41,050 casos:

- programas reales y mutaciones aleatorias de los mismos;
- casos límite escritos a mano: secuencias de escape como `"\u002b"`, espacios en blanco, enteros grandes, anidamiento profundo, `NaN`.

No discreparon en ninguno de ellos.

## Línea de comandos

```bash
cargo build --release
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv            # veredicto, puntuación, pasos muertos
./target/release/elfcalc check --puzzles puzzles.csv --sub submission.csv --dce out.csv   # escribe una versión sin pasos muertos
./target/release/elfcalc bench --puzzles puzzles.csv --sub submission.csv --reps 20
```

`check` informa de filas duplicadas, desconocidas, faltantes o mal formadas, y sale con código 1 ante cualquier problema. Está diseñado para la velocidad. Para la comprobación final antes de subir tu envío, ejecuta también el verificador CSV byte a byte [santa-2026-submission-verifier](https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier).

## Biblioteca

```rust
use elfcalc::{check_cell, parse_cell, strip_dead, Puzzle};

let p = Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 };
let cell = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;
assert_eq!(check_cell(&p, cell), Ok(4));                 // válido, 4 instrucciones
let prog = parse_cell(cell, p.registers).unwrap();
let shorter = strip_dead(&p, &prog);                      // nunca más largo, siempre válido
```

`check` y `check_cell` validan primero el puzle y nunca entran en pánico (`panic`). `strip_dead` elimina las instrucciones que no conducen a la primera aparición de ningún objetivo. El resultado es siempre válido y nunca más largo. No siempre es mínimo: una cadena muerta que casualmente vuelve a calcular un objetivo ya cubierto se conserva.

## Entorno interactivo

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown --features ffi
python3 playground/build.py target/wasm32-unknown-unknown/release/elfcalc.wasm index.html --standalone
```

`index.html` es un único archivo autocontenido que funciona sin conexión. La ABI de C que usa la página está protegida tras la característica `ffi`, por lo que la biblioteca de Rust normal no exporta ningún símbolo global.

## Licencia

MIT. Consulta [LICENSE](LICENSE). El entorno interactivo carga sus fuentes (Fraunces, Plus Jakarta Sans, JetBrains Mono, Noto Sans CJK) desde Google Fonts, bajo la SIL Open Font License.

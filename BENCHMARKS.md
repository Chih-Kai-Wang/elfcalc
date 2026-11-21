# Replaying Elf Calculator programs in Python, Rust and Goose

The task: replay 1,000 programs (132,635 instructions in total), check every step against the rules (positive
subtraction, exact division, values at or below a limit of up to 10^18, targets covered), and report the score.

The three versions use the same algorithm on the same pre-parsed input, a flat list of integers:

- 48 fixed registers;
- the targets as a bitmask;
- every multiplication checked as `a > limit / b` before it is done. Additions can't wrap because the benchmark
  requires `value_limit <= 10^18`, and the competition's limits are all below that. `examples/flat.rs` treats any
  puzzle outside these preconditions as illegal.

The three are `goose/replay.goose`, `examples/flat.rs` and `py_flat` in `bench/three_lang_bench.py`. That script
also writes the inputs from a submission file, since the competition data isn't included here.

All three give the same score, failure count and a per-puzzle verdict signature:

- on the 1,000 legal programs;
- on 5,000 programs that include 4 random mutations of each, of which 3,659 are illegal or incomplete.

The reference for both runs is the competition verifier's `check_program`.

Measured on an Apple-silicon laptop (macOS, Rust 1.100, Goose v2026.4 with clang 21, CPython 3.9), 200
repetitions, check time only (parsing excluded):

| Implementation | ms per 1,000 programs | relative to Rust, no overflow traps |
|---|---:|---:|
| Python, same flat algorithm | 96.6 | 79× slower |
| Python, the verifier's `check_program` (sets, lists) | 43.9 | 36× slower |
| Rust, `overflow-checks = true` | 2.2 | 1.8× slower |
| Goose v2026.4 release build | 1.39–1.52 | 1.1–1.3× slower |
| Rust, `overflow-checks = false` | 1.20–1.24 | 1× |

What we learned:

- **Overflow traps are the biggest single cost in the Rust version.**
  - Turning them on makes the hot loop about 1.8× slower.
  - This replay never relies on them, because every operation is range-checked before it runs.
  - Goose's release builds don't trap on overflow either, unsigned arithmetic wraps there, so the like-for-like
    comparison is with traps off.
- **With equal settings, Rust is slightly faster here (about 15%).**
  - Goose is within the same small range and clearly ahead of Rust with traps on.
  - Its memory model (no heap, fixed arrays inline) doesn't help much in this workload, which allocates nothing
    in the hot loop.
- **The idiomatic Python verifier is about 2× faster than the same flat loop in Python.**
  - The verifier spends its time in C-implemented set and list operations.
  - The flat loop spends it in bytecode.

**About the library itself.** `src/lib.rs` is a different implementation, so it is left out of the table:

- it parses JSON cells;
- it keeps targets in a sorted list;
- it allocates per program;
- it uses u128 for intermediate products.

On the same submission, `elfcalc bench` reports about 11 ms per file for parse + check and 1.7 ms for check only.
`[profile.release]` settings apply only when elfcalc is the top-level crate. Code that depends on the library uses
its own profile, so the library doesn't rely on overflow traps for correctness: every operation is range-checked
or done in u128.

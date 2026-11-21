//! Same algorithm as goose/replay.goose and scripts .../flat_replay.py, for a like-for-like benchmark.
//! Usage: cargo run --release --example flat -- NUMS_FILE [reps]
//! Preconditions (checked): value_limit <= 10^18 (so a + b cannot wrap), at most 64 distinct targets per puzzle,
//! op codes 0..=3, registers 0..=47. Anything else counts as an illegal program.
use std::time::Instant;

fn check_all(nums: &[u64]) -> (u64, u64, u64) {
    let mut pos = 0usize;
    let (mut total, mut bad, mut sig) = (0u64, 0u64, 0u64);
    let np = nums[pos] as usize;
    pos += 1;
    for p in 0..np {
        let mut regs = [0u64; 48];
        let ns = nums[pos] as usize;
        pos += 1;
        regs[..ns].copy_from_slice(&nums[pos..pos + ns]);
        pos += ns;
        let nt = nums[pos] as usize;
        pos += 1;
        let tg = &nums[pos..pos + nt];
        pos += nt;
        let limit = nums[pos];
        let ni = nums[pos + 1] as usize;
        pos += 2;
        if nt > 64 || limit > 1_000_000_000_000_000_000 || ns > 48 {
            pos += 4 * ni; // outside this benchmark's preconditions: count as illegal
            bad += 1;
            continue;
        }
        let mut cov = 0u64;
        for (t, &v) in tg.iter().enumerate() {
            if regs[..ns].contains(&v) {
                cov |= 1 << t;
            }
        }
        let mut ok = true;
        for _ in 0..ni {
            let (op, i, j, d) = (nums[pos], nums[pos + 1] as usize, nums[pos + 2] as usize, nums[pos + 3] as usize);
            pos += 4;
            if !ok {
                continue;
            }
            if op > 3 || i >= 48 || j >= 48 || d >= 48 {
                ok = false;
                continue;
            }
            let (a, b) = (regs[i], regs[j]);
            if a == 0 || b == 0 {
                ok = false;
                continue;
            }
            let r = match op {
                0 => a + b,
                1 => {
                    if a <= b { ok = false; continue; }
                    a - b
                }
                2 => {
                    if a > limit / b { ok = false; continue; }
                    a * b
                }
                _ => {
                    if a % b != 0 { ok = false; continue; }
                    a / b
                }
            };
            if r > limit {
                ok = false;
                continue;
            }
            regs[d] = r;
            for (t, &v) in tg.iter().enumerate() {
                if v == r {
                    cov |= 1 << t;
                }
            }
        }
        let full = if nt == 64 { u64::MAX } else { (1u64 << nt) - 1 };
        if ok && cov == full {
            total += ni as u64;
            sig = sig.wrapping_add((p as u64 + 1).wrapping_mul(ni as u64));
        } else {
            bad += 1;
        }
    }
    (total, bad, sig)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1]).expect("read");
    let reps: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let t0 = Instant::now();
    let nums: Vec<u64> = text.split_ascii_whitespace().map(|x| x.parse().unwrap()).collect();
    let t1 = Instant::now();
    let mut res = (0, 0, 0);
    for _ in 0..reps {
        res = check_all(std::hint::black_box(&nums));
    }
    let t2 = Instant::now();
    println!("rust:  score={} bad={} sig={} parse {:.3} ms, check {:.3} ms/file", res.0, res.1, res.2,
        (t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3 / reps as f64);
}

//! elfcalc command line. A fast checker and dead-step remover; for the final pre-submission check use the
//! strict verifier (santa-2026-submission-verifier), which also checks the CSV byte by byte.
//!   elfcalc check  --puzzles puzzles.csv --sub submission.csv [--dce out.csv]
//!   elfcalc bench  --puzzles puzzles.csv --sub submission.csv [--reps 20]
//!   elfcalc cells  FIXTURE            (one case per line: start;targets;limit;registers<TAB>hex(cell))
use elfcalc::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const USAGE: &str = "usage: elfcalc check --puzzles P --sub S [--dce OUT]\n       elfcalc bench --puzzles P --sub S [--reps N]\n       elfcalc cells FIXTURE";

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}\n{USAGE}");
    std::process::exit(2)
}

/// Parse `--flag value` pairs; unknown flags, missing values and repeated flags are errors.
fn flags(args: &[String], allowed: &[&str]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut k = 0;
    while k < args.len() {
        let a = &args[k];
        let (name, value) = match a.split_once('=') {
            Some((n, v)) if a.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        if !allowed.contains(&name.as_str()) {
            die(&format!("unknown argument {a}"));
        }
        let value = match value {
            Some(v) => v,
            None => {
                k += 1;
                match args.get(k) {
                    Some(v) if !v.starts_with("--") => v.clone(),
                    _ => die(&format!("{name} needs a value")),
                }
            }
        };
        if out.insert(name.clone(), value).is_some() {
            die(&format!("{name} given twice"));
        }
        k += 1;
    }
    out
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| die(&format!("cannot read {path}: {e}")))
}

struct Loaded {
    puzzles: Vec<(String, Puzzle)>,
    cells: HashMap<String, String>,
    problems: Vec<String>,
}

fn load(f: &HashMap<String, String>) -> Loaded {
    let pz_path = f.get("--puzzles").unwrap_or_else(|| die("--puzzles is required"));
    let sub_path = f.get("--sub").unwrap_or_else(|| die("--sub is required"));
    let puzzles = load_puzzles(&read(pz_path)).unwrap_or_else(|e| die(&format!("{pz_path}: {e}")));
    let text = read(sub_path);
    let rows = read_csv(text.trim_start_matches('\u{feff}')).unwrap_or_else(|| die(&format!("{sub_path}: malformed CSV")));
    if rows.first().map(|h| h.as_slice() != ["id", "program"]).unwrap_or(true) {
        die(&format!("{sub_path}: header must be id,program"));
    }
    let known: HashSet<&str> = puzzles.iter().map(|(id, _)| id.as_str()).collect();
    let mut cells = HashMap::new();
    let mut problems = Vec::new();
    for (n, r) in rows.iter().enumerate().skip(1) {
        if r.len() != 2 {
            problems.push(format!("row {}: expected 2 columns (id,program), got {}", n + 1, r.len()));
            continue;
        }
        if !known.contains(r[0].as_str()) {
            problems.push(format!("row {}: unknown id {:?}", n + 1, r[0]));
            continue;
        }
        if cells.insert(r[0].clone(), r[1].clone()).is_some() {
            problems.push(format!("row {}: duplicate id {:?}", n + 1, r[0]));
        }
    }
    Loaded { puzzles, cells, problems }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("check") => {
            let f = flags(&args[2..], &["--puzzles", "--sub", "--dce"]);
            let l = load(&f);
            let mut problems = l.problems.clone();
            let t = Instant::now();
            let mut total = 0usize;
            let mut progs: Vec<(&String, &Puzzle, Vec<Instr>)> = Vec::new();
            for (id, p) in &l.puzzles {
                match l.cells.get(id) {
                    None => problems.push(format!("{id}: missing")),
                    Some(cell) => match parse_cell(cell, p.registers).and_then(|prog| check(p, &prog).map(|_| prog)) {
                        Ok(prog) => {
                            total += prog.len();
                            progs.push((id, p, prog));
                        }
                        Err(e) => problems.push(format!("{id}: {e}")),
                    },
                }
            }
            let check_ms = t.elapsed().as_secs_f64() * 1e3;
            for e in problems.iter().take(20) {
                println!("ERROR {e}");
            }
            if problems.len() > 20 {
                println!("ERROR ... and {} more", problems.len() - 20);
            }
            if !problems.is_empty() {
                println!("REJECTED errors={}", problems.len());
                if f.contains_key("--dce") {
                    println!("(no --dce output written: fix the errors first)");
                }
                std::process::exit(1);
            }
            let stripped: Vec<(&String, Vec<Instr>)> = progs.iter().map(|(id, p, prog)| (*id, strip_dead(p, prog))).collect();
            let dead: usize = progs.iter().zip(&stripped).map(|((_, _, a), (_, b))| a.len() - b.len()).sum();
            println!("VALID puzzles={} score={total} dead_instructions={dead} (checked in {check_ms:.1} ms)", l.puzzles.len());
            if let Some(path) = f.get("--dce") {
                let mut s = String::from("id,program\n");
                for (id, prog) in &stripped {
                    s.push_str(&format!("{},{}\n", csv_field(id), csv_field(&to_json(prog))));
                }
                std::fs::write(path, s).unwrap_or_else(|e| die(&format!("cannot write {path}: {e}")));
                println!("wrote {path} (score {})", total - dead);
            }
        }
        Some("bench") => {
            let f = flags(&args[2..], &["--puzzles", "--sub", "--reps"]);
            let reps: usize = match f.get("--reps") {
                None => 20,
                Some(v) => match v.parse() {
                    Ok(n) if n > 0 => n,
                    _ => die("--reps must be a positive integer"),
                },
            };
            let l = load(&f);
            let progs: Vec<(&Puzzle, &str)> =
                l.puzzles.iter().filter_map(|(id, p)| l.cells.get(id).map(|c| (p, c.as_str()))).collect();
            let parsed: Vec<(&Puzzle, Vec<Instr>)> =
                progs.iter().filter_map(|(p, c)| parse_cell(c, p.registers).ok().map(|v| (*p, v))).collect();
            if parsed.len() != progs.len() || progs.is_empty() {
                die("bench needs a submission whose every cell parses");
            }
            let n_instr: usize = parsed.iter().map(|(_, v)| v.len()).sum::<usize>().max(1);
            let t = Instant::now();
            let mut acc = 0usize;
            for _ in 0..reps {
                for (p, c) in std::hint::black_box(&progs) {
                    acc += check_cell(p, c).unwrap_or(0);
                }
            }
            let parse_check = t.elapsed().as_secs_f64() / reps as f64;
            let t = Instant::now();
            for _ in 0..reps {
                for (p, v) in std::hint::black_box(&parsed) {
                    acc += check(p, v).unwrap_or(0);
                }
            }
            let check_only = t.elapsed().as_secs_f64() / reps as f64;
            println!(
                "programs={} instructions={n_instr} parse+check {:.2} ms/file ({:.0} ns/instr), check only {:.2} ms/file ({:.0} ns/instr) [{acc}]",
                progs.len(),
                parse_check * 1e3,
                parse_check * 1e9 / n_instr as f64,
                check_only * 1e3,
                check_only * 1e9 / n_instr as f64
            );
        }
        Some("cells") => {
            let path = args.get(2).unwrap_or_else(|| die("cells needs a FIXTURE path"));
            for (n, line) in read(path).lines().enumerate() {
                let bad = |why: &str| -> ! { die(&format!("{path}:{}: {why}", n + 1)) };
                let (head, hex) = line.split_once('\t').unwrap_or_else(|| bad("expected PUZZLE<TAB>HEX"));
                let f: Vec<&str> = head.split(';').collect();
                if f.len() != 4 {
                    bad("puzzle must be start;targets;limit;registers");
                }
                let nums = |s: &str| -> Vec<u64> {
                    if s.is_empty() { vec![] } else { s.split(',').map(|x| parse_digits(x).unwrap_or_else(|| bad("bad number"))).collect() }
                };
                let p = Puzzle {
                    start: nums(f[0]),
                    targets: nums(f[1]),
                    limit: parse_digits(f[2]).unwrap_or_else(|| bad("bad limit")),
                    registers: parse_digits(f[3]).unwrap_or_else(|| bad("bad registers")) as usize,
                };
                if hex.len() % 2 != 0 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                    bad("cell must be hex");
                }
                let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|k| u8::from_str_radix(&hex[k..k + 2], 16).unwrap()).collect();
                match String::from_utf8(bytes) {
                    Ok(cell) => match check_cell(&p, &cell) {
                        Ok(n) => println!("OK {n}"),
                        Err(e) => println!("ERR {e}"),
                    },
                    Err(_) => println!("ERR not utf-8"),
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

//! elfcalc: exact, fast replay of Elf Calculator programs (Kaggle "Santa 2026 - The Elf Calculator").
//!
//! * [`parse_cell`] reads one submission cell (the JSON program text) and accepts exactly the cells a strict
//!   checker accepts: an array of `[op, i, j, d]` with `op` one of `+ - * /` (JSON string escapes allowed) and
//!   `i, j, d` plain non-negative JSON integers. Anything else (floats, booleans, `-0`, nesting, invalid JSON)
//!   is rejected.
//! * [`check`] replays a program on a puzzle with exact integer arithmetic (u128 for intermediates, so
//!   nothing can wrap) and reports the instruction count or the first violated rule.
//! * [`analyze`] extracts the value DAG and marks dead instructions; [`strip_dead`] removes them.
//!
//! No dependencies. All arithmetic is checked.

pub const NREG: usize = 48;
pub const MAX_INSTR: usize = 4096;
pub const MAX_BYTES: usize = 131_072;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    pub fn symbol(self) -> char {
        match self {
            Op::Add => '+',
            Op::Sub => '-',
            Op::Mul => '*',
            Op::Div => '/',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instr {
    pub op: Op,
    pub i: usize,
    pub j: usize,
    pub d: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Puzzle {
    pub start: Vec<u64>,
    pub targets: Vec<u64>,
    pub limit: u64,
    pub registers: usize,
}

/// Largest register file accepted from untrusted puzzle input (the competition uses 48).
pub const MAX_REGISTERS: usize = 4096;

impl Puzzle {
    /// Check the puzzle itself: 1..=MAX_REGISTERS registers, 1 <= len(start) <= registers, positive start
    /// values and targets, limit >= 1. `check` and `analyze` call this first, so they never panic.
    pub fn validate(&self) -> Result<(), Reject> {
        if self.registers == 0 || self.registers > MAX_REGISTERS {
            return Err(Reject::InvalidPuzzle("registers must be 1..=4096"));
        }
        if self.start.is_empty() || self.start.len() > self.registers {
            return Err(Reject::InvalidPuzzle("need 1 <= starting values <= registers"));
        }
        if self.start.iter().chain(&self.targets).any(|&v| v == 0) {
            return Err(Reject::InvalidPuzzle("starting values and targets must be positive"));
        }
        if self.limit == 0 {
            return Err(Reject::InvalidPuzzle("limit must be >= 1"));
        }
        Ok(())
    }
}

/// Why a cell or program was rejected. `step` is 0-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    CellTooLarge(usize),
    BadJson(usize),
    BadInstruction { step: usize },
    BadOp { step: usize },
    BadRegister { step: usize },
    TooManyInstructions(usize),
    EmptyRegister { step: usize, reg: usize },
    NonPositiveSubtraction { step: usize },
    InexactDivision { step: usize },
    OverLimit { step: usize },
    NotCovered(usize),
    DivisionByZero { step: usize },
    InvalidPuzzle(&'static str),
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reject::CellTooLarge(n) => write!(f, "cell is {n} bytes > {MAX_BYTES}"),
            Reject::BadJson(at) => write!(f, "not a program array (JSON problem near byte {at})"),
            Reject::BadInstruction { step } => write!(f, "step {step}: instruction is not [op, i, j, d]"),
            Reject::BadOp { step } => write!(f, "step {step}: bad op (use one of + - * /)"),
            Reject::BadRegister { step } => write!(f, "step {step}: bad register"),
            Reject::TooManyInstructions(n) => write!(f, "too many instructions ({n} > {MAX_INSTR})"),
            Reject::EmptyRegister { step, reg } => write!(f, "step {step}: reads empty register r{reg}"),
            Reject::NonPositiveSubtraction { step } => write!(f, "step {step}: non-positive subtraction"),
            Reject::InexactDivision { step } => write!(f, "step {step}: inexact division"),
            Reject::OverLimit { step } => write!(f, "step {step}: value exceeds limit"),
            Reject::NotCovered(n) => write!(f, "{n} targets not covered"),
            Reject::DivisionByZero { step } => write!(f, "step {step}: division by zero"),
            Reject::InvalidPuzzle(why) => write!(f, "invalid puzzle: {why}"),
        }
    }
}

// ---------------------------------------------------------------- strict cell parser

struct P<'a> {
    b: &'a [u8],
    k: usize,
}

/// Register indices bigger than this are rejected without overflowing.
const REG_CAP: u64 = 1 << 32;

enum Elem {
    Op(Option<Op>), // a JSON string: Some if it is exactly one of + - * /
    Int(u64),       // a plain non-negative integer (saturated at REG_CAP)
    Other,          // any other valid JSON value
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.k < self.b.len() && matches!(self.b[self.k], b' ' | b'\t' | b'\n' | b'\r') {
            self.k += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.k < self.b.len() && self.b[self.k] == c {
            self.k += 1;
            true
        } else {
            false
        }
    }
    fn hex4(&mut self) -> Option<u32> {
        if self.k + 4 > self.b.len() {
            return None;
        }
        let raw = &self.b[self.k..self.k + 4];
        if !raw.iter().all(|c| c.is_ascii_hexdigit()) {
            return None; // u32::from_str_radix would accept a leading '+'
        }
        let s = std::str::from_utf8(raw).ok()?;
        let v = u32::from_str_radix(s, 16).ok()?;
        self.k += 4;
        Some(v)
    }
    /// JSON string -> its decoded chars (only needed to recognise one-character ops).
    fn string(&mut self) -> Option<Vec<u32>> {
        // at opening quote
        self.k += 1;
        let mut out = Vec::new();
        loop {
            let c = *self.b.get(self.k)?;
            self.k += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = *self.b.get(self.k)?;
                    self.k += 1;
                    let v = match e {
                        b'"' => '"' as u32,
                        b'\\' => '\\' as u32,
                        b'/' => '/' as u32,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => self.hex4()?,
                        _ => return None,
                    };
                    out.push(v);
                }
                0..=0x1f => return None, // control characters must be escaped
                _ => {
                    // count one char per UTF-8 lead byte (continuation bytes are skipped); exact decoding is irrelevant
                    if c & 0xC0 != 0x80 {
                        out.push(if c < 0x80 { c as u32 } else { 0xFFFD });
                    }
                }
            }
        }
    }
    fn number(&mut self) -> Option<Elem> {
        let s = self.k;
        if self.b.get(self.k) == Some(&b'-') {
            self.k += 1;
        }
        let d0 = self.k;
        match self.b.get(self.k) {
            Some(b'0') => self.k += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.b.get(self.k), Some(b'0'..=b'9')) {
                    self.k += 1;
                }
            }
            _ => return None,
        }
        let int_end = self.k;
        let mut is_int = true;
        if self.b.get(self.k) == Some(&b'.') {
            self.k += 1;
            if !matches!(self.b.get(self.k), Some(b'0'..=b'9')) {
                return None;
            }
            while matches!(self.b.get(self.k), Some(b'0'..=b'9')) {
                self.k += 1;
            }
            is_int = false;
        }
        if matches!(self.b.get(self.k), Some(b'e' | b'E')) {
            self.k += 1;
            if matches!(self.b.get(self.k), Some(b'+' | b'-')) {
                self.k += 1;
            }
            if !matches!(self.b.get(self.k), Some(b'0'..=b'9')) {
                return None;
            }
            while matches!(self.b.get(self.k), Some(b'0'..=b'9')) {
                self.k += 1;
            }
            is_int = false;
        }
        if !is_int || d0 != s {
            return Some(Elem::Other); // float, exponent or negative (incl. -0): never a legal register
        }
        let mut v: u64 = 0;
        for &c in &self.b[d0..int_end] {
            v = (v * 10 + (c - b'0') as u64).min(REG_CAP);
        }
        Some(Elem::Int(v))
    }
    fn literal(&mut self, word: &[u8]) -> Option<()> {
        if self.b[self.k..].starts_with(word) {
            self.k += word.len();
            Some(())
        } else {
            None
        }
    }
    /// Any JSON value; arrays/objects are validated recursively (depth-limited) and reported as Other.
    fn value(&mut self, depth: usize) -> Option<Elem> {
        if depth > 512 {
            return None;
        }
        self.ws();
        match *self.b.get(self.k)? {
            b'"' => {
                let s = self.string()?;
                let op = match s.as_slice() {
                    [c] if *c == '+' as u32 => Some(Op::Add),
                    [c] if *c == '-' as u32 => Some(Op::Sub),
                    [c] if *c == '*' as u32 => Some(Op::Mul),
                    [c] if *c == '/' as u32 => Some(Op::Div),
                    _ => None,
                };
                Some(Elem::Op(op))
            }
            b'[' => {
                self.k += 1;
                if !self.eat(b']') {
                    loop {
                        self.value(depth + 1)?;
                        if self.eat(b']') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                Some(Elem::Other)
            }
            b'{' => {
                self.k += 1;
                if !self.eat(b'}') {
                    loop {
                        self.ws();
                        if self.b.get(self.k) != Some(&b'"') {
                            return None;
                        }
                        self.string()?;
                        if !self.eat(b':') {
                            return None;
                        }
                        self.value(depth + 1)?;
                        if self.eat(b'}') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                Some(Elem::Other)
            }
            b't' => self.literal(b"true").map(|_| Elem::Other),
            b'f' => self.literal(b"false").map(|_| Elem::Other),
            b'n' => self.literal(b"null").map(|_| Elem::Other),
            b'N' => self.literal(b"NaN").map(|_| Elem::Other), // Python's json accepts these
            b'I' => self.literal(b"Infinity").map(|_| Elem::Other),
            b'-' if self.b[self.k..].starts_with(b"-Infinity") => {
                self.k += 9;
                Some(Elem::Other)
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }
}

/// One parsed element of an instruction array.
enum Slot {
    Op(Option<Op>),
    Int(u64),
    Other,
}

/// Parse one submission cell. Returns the program, or the first reason it cannot be a legal program.
/// Register indices are checked against `nreg` here (as a strict checker does before replaying).
pub fn parse_cell(cell: &str, nreg: usize) -> Result<Vec<Instr>, Reject> {
    if cell.len() > MAX_BYTES {
        return Err(Reject::CellTooLarge(cell.len()));
    }
    let mut p = P { b: cell.as_bytes(), k: 0 };
    let bad = |p: &P| Reject::BadJson(p.k);
    if !p.eat(b'[') {
        // a valid JSON value that is not an array is "not a program"; invalid JSON likewise rejected
        return Err(bad(&p));
    }
    let mut prog = Vec::new();
    // First pass collects instruction shapes; a structural error anywhere (even after a bad instruction)
    // is still just a rejection, so we can stop at the first problem.
    let mut first_err: Option<Reject> = None;
    if !p.eat(b']') {
        loop {
            p.ws();
            let step = prog.len() + if first_err.is_some() { 1 } else { 0 };
            if p.b.get(p.k) == Some(&b'[') {
                // an instruction candidate: read up to 5 elements to know its arity
                p.k += 1;
                let mut slots: Vec<Slot> = Vec::with_capacity(4);
                if !p.eat(b']') {
                    loop {
                        let e = p.value(1).ok_or_else(|| bad(&p))?;
                        slots.push(match e {
                            Elem::Op(o) => Slot::Op(o),
                            Elem::Int(v) => Slot::Int(v),
                            Elem::Other => Slot::Other,
                        });
                        if p.eat(b']') {
                            break;
                        }
                        if !p.eat(b',') {
                            return Err(bad(&p));
                        }
                    }
                }
                if first_err.is_none() {
                    match instr_from(&slots, nreg, step) {
                        Ok(ins) => prog.push(ins),
                        Err(r) => first_err = Some(r),
                    }
                }
            } else {
                p.value(1).ok_or_else(|| bad(&p))?;
                if first_err.is_none() {
                    first_err = Some(Reject::BadInstruction { step });
                }
            }
            if p.eat(b']') {
                break;
            }
            if !p.eat(b',') {
                return Err(bad(&p));
            }
        }
    }
    p.ws();
    if p.k != p.b.len() {
        return Err(bad(&p));
    }
    if let Some(r) = first_err {
        return Err(r);
    }
    if prog.len() > MAX_INSTR {
        return Err(Reject::TooManyInstructions(prog.len()));
    }
    Ok(prog)
}

fn instr_from(slots: &[Slot], nreg: usize, step: usize) -> Result<Instr, Reject> {
    if slots.len() != 4 {
        return Err(Reject::BadInstruction { step });
    }
    let op = match slots[0] {
        Slot::Op(Some(o)) => o,
        _ => return Err(Reject::BadOp { step }),
    };
    let mut r = [0usize; 3];
    for (k, s) in slots[1..].iter().enumerate() {
        match *s {
            Slot::Int(v) if (v as usize) < nreg && v < REG_CAP => r[k] = v as usize,
            _ => return Err(Reject::BadRegister { step }),
        }
    }
    Ok(Instr { op, i: r[0], j: r[1], d: r[2] })
}

// ---------------------------------------------------------------- replay

/// Result value of one instruction, or the rule it breaks.
#[inline]
pub fn apply(op: Op, a: u64, b: u64, limit: u64, step: usize) -> Result<u64, Reject> {
    let (a, b) = (a as u128, b as u128);
    let r: u128 = match op {
        Op::Add => a + b,
        Op::Sub => {
            if a <= b {
                return Err(Reject::NonPositiveSubtraction { step });
            }
            a - b
        }
        Op::Mul => a * b, // both < 2^64, so the product fits in u128
        Op::Div => {
            if b == 0 {
                return Err(Reject::DivisionByZero { step });
            }
            if a % b != 0 {
                return Err(Reject::InexactDivision { step });
            }
            a / b
        }
    };
    if r == 0 || r > limit as u128 {
        return Err(Reject::OverLimit { step });
    }
    Ok(r as u64)
}

/// Replay `prog` on `puz`; Ok(instruction count) iff every instruction is legal and every target is covered.
pub fn check(puz: &Puzzle, prog: &[Instr]) -> Result<usize, Reject> {
    puz.validate()?;
    if prog.len() > MAX_INSTR {
        return Err(Reject::TooManyInstructions(prog.len()));
    }
    let mut regs: Vec<Option<u64>> = vec![None; puz.registers];
    for (k, &v) in puz.start.iter().enumerate() {
        regs[k] = Some(v);
    }
    let mut need: Vec<u64> = puz.targets.clone();
    need.sort_unstable();
    need.dedup();
    let mut covered = vec![false; need.len()];
    let mut left = need.len();
    let cover = |v: u64, covered: &mut Vec<bool>, left: &mut usize| {
        if let Ok(k) = need.binary_search(&v) {
            if !covered[k] {
                covered[k] = true;
                *left -= 1;
            }
        }
    };
    for v in puz.start.iter() {
        cover(*v, &mut covered, &mut left);
    }
    for (step, ins) in prog.iter().enumerate() {
        if ins.i >= puz.registers || ins.j >= puz.registers || ins.d >= puz.registers {
            return Err(Reject::BadRegister { step });
        }
        let a = regs[ins.i].ok_or(Reject::EmptyRegister { step, reg: ins.i })?;
        let b = regs[ins.j].ok_or(Reject::EmptyRegister { step, reg: ins.j })?;
        let r = apply(ins.op, a, b, puz.limit, step)?;
        regs[ins.d] = Some(r);
        cover(r, &mut covered, &mut left);
    }
    if left > 0 {
        return Err(Reject::NotCovered(left));
    }
    Ok(prog.len())
}

/// Parse and check one cell: the verdict a strict submission checker gives for this puzzle.
pub fn check_cell(puz: &Puzzle, cell: &str) -> Result<usize, Reject> {
    puz.validate()?;
    let prog = parse_cell(cell, puz.registers)?;
    check(puz, &prog)
}

// ---------------------------------------------------------------- analysis

/// Per-instruction facts from a replay of a legal program.
#[derive(Clone, Debug)]
pub struct StepInfo {
    pub value: u64,
    /// this step is the first time a target value appears
    pub first_target_hit: bool,
    /// the value written here is read by a later instruction before being overwritten
    pub read_later: bool,
    /// the step contributes to covering some target (directly or through later reads)
    pub live: bool,
}

/// Replay a legal program and classify each instruction. Returns None if the program is illegal.
/// Liveness is rooted at the first step that produces each target, so a dead chain that happens to recompute an
/// already-covered target later is kept; `strip_dead` is sound (never longer, never illegal) but not minimal.
pub fn analyze(puz: &Puzzle, prog: &[Instr]) -> Option<Vec<StepInfo>> {
    check(puz, prog).ok()?;
    let mut regs: Vec<Option<u64>> = vec![None; puz.registers];
    for (k, &v) in puz.start.iter().enumerate() {
        regs[k] = Some(v);
    }
    let mut seen: std::collections::HashSet<u64> = puz.start.iter().copied().collect();
    let targets: std::collections::HashSet<u64> = puz.targets.iter().copied().collect();
    // writer[r] = step that produced the current content of register r (None = start value)
    let mut writer: Vec<Option<usize>> = vec![None; puz.registers];
    let mut info = Vec::with_capacity(prog.len());
    let mut deps: Vec<[Option<usize>; 2]> = Vec::with_capacity(prog.len());
    for (step, ins) in prog.iter().enumerate() {
        let (a, b) = (regs[ins.i].unwrap(), regs[ins.j].unwrap());
        let v = apply(ins.op, a, b, puz.limit, step).ok()?;
        deps.push([writer[ins.i], writer[ins.j]]);
        for w in [writer[ins.i], writer[ins.j]].into_iter().flatten() {
            let s: &mut StepInfo = &mut info[w];
            s.read_later = true;
        }
        let first = targets.contains(&v) && seen.insert(v);
        info.push(StepInfo { value: v, first_target_hit: first, read_later: false, live: false });
        regs[ins.d] = Some(v);
        writer[ins.d] = Some(step);
    }
    // liveness: roots are first target hits; propagate backward through the operands they read
    for step in (0..prog.len()).rev() {
        if info[step].first_target_hit {
            info[step].live = true;
        }
        if info[step].live {
            for w in deps[step].into_iter().flatten() {
                info[w].live = true;
            }
        }
    }
    Some(info)
}

/// Remove instructions that do not contribute to covering any target. The result is never longer and,
/// for a legal program, still legal (verified before returning; on any doubt the input is returned).
pub fn strip_dead(puz: &Puzzle, prog: &[Instr]) -> Vec<Instr> {
    let Some(info) = analyze(puz, prog) else { return prog.to_vec() };
    let kept: Vec<Instr> = prog.iter().zip(info.iter()).filter(|(_, s)| s.live).map(|(i, _)| *i).collect();
    if kept.len() < prog.len() && check(puz, &kept).is_ok() {
        kept
    } else {
        prog.to_vec()
    }
}

/// Compact canonical JSON for a program, as `json.dumps(prog, separators=(",", ":"))` writes it.
pub fn to_json(prog: &[Instr]) -> String {
    let mut s = String::with_capacity(prog.len() * 14 + 2);
    s.push('[');
    for (k, ins) in prog.iter().enumerate() {
        if k > 0 {
            s.push(',');
        }
        s.push_str(&format!("[\"{}\",{},{},{}]", ins.op.symbol(), ins.i, ins.j, ins.d));
    }
    s.push(']');
    s
}

// ---------------------------------------------------------------- tiny CSV reader (RFC 4180 subset)

/// Read all records of a CSV text (comma separated, double-quote quoting, CRLF or LF). Strict: text after a
/// closing quote, a quote inside an unquoted field, or a bare CR is an error. Not a submission gate: use the
/// pinned verifier for that. Returns None on malformed input.
pub fn read_csv(text: &str) -> Option<Vec<Vec<String>>> {
    let b = text.as_bytes();
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field: Vec<u8> = Vec::new();
    let (mut quoted, mut closed, mut started) = (false, false, false);
    let mut k = 0;
    let end_field = |row: &mut Vec<String>, field: &mut Vec<u8>| -> Option<()> {
        row.push(String::from_utf8(std::mem::take(field)).ok()?);
        Some(())
    };
    while k < b.len() {
        let c = b[k];
        if quoted {
            if c == b'"' {
                if b.get(k + 1) == Some(&b'"') {
                    field.push(b'"');
                    k += 1;
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            k += 1;
            continue;
        }
        match c {
            b'"' if !started => {
                quoted = true;
                started = true;
            }
            b',' => {
                end_field(&mut row, &mut field)?;
                closed = false;
                started = false;
            }
            b'\r' if b.get(k + 1) == Some(&b'\n') => {}
            b'\n' => {
                end_field(&mut row, &mut field)?;
                rows.push(std::mem::take(&mut row));
                closed = false;
                started = false;
            }
            _ if closed => return None,
            b'"' | b'\r' => return None,
            _ => {
                field.push(c);
                started = true;
            }
        }
        k += 1;
    }
    if quoted {
        return None;
    }
    if started || !row.is_empty() {
        end_field(&mut row, &mut field)?;
        rows.push(row);
    }
    Some(rows)
}

/// A plain unsigned decimal (digits only: `str::parse` would also accept a leading '+').
pub fn parse_digits(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn int_list(s: &str) -> Option<Vec<u64>> {
    let t = s.trim().strip_prefix('[')?.strip_suffix(']')?;
    if t.trim().is_empty() {
        return Some(vec![]);
    }
    t.split(',').map(parse_digits).collect()
}

/// Load a puzzles CSV (id,starting_values,targets,value_limit,registers,...) into (id, Puzzle) pairs.
pub fn load_puzzles(text: &str) -> Result<Vec<(String, Puzzle)>, String> {
    let rows = read_csv(text.trim_start_matches('\u{feff}')).ok_or("malformed CSV")?;
    let h = rows.first().ok_or("empty puzzles file")?;
    let col = |name: &str| h.iter().position(|c| c == name).ok_or(format!("missing column {name}"));
    let (ci, cs, ct, cl, cr) = (col("id")?, col("starting_values")?, col("targets")?, col("value_limit")?, col("registers")?);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (n, r) in rows.iter().enumerate().skip(1) {
        if r.len() < h.len() {
            return Err(format!("row {} is short", n + 1));
        }
        let bad = || format!("row {} has a malformed field", n + 1);
        let p = Puzzle {
            start: int_list(&r[cs]).ok_or_else(bad)?,
            targets: int_list(&r[ct]).ok_or_else(bad)?,
            limit: parse_digits(&r[cl]).ok_or_else(bad)?,
            registers: parse_digits(&r[cr]).ok_or_else(bad)? as usize,
        };
        p.validate().map_err(|e| format!("row {}: {e}", n + 1))?;
        if !seen.insert(r[ci].clone()) {
            return Err(format!("row {}: duplicate puzzle id {:?}", n + 1, r[ci]));
        }
        out.push((r[ci].clone(), p));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex() -> Puzzle {
        Puzzle { start: vec![3], targets: vec![1, 2, 5, 8], limit: 9, registers: 48 }
    }
    const GOOD: &str = r#"[["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#;

    #[test]
    fn worked_example() {
        assert_eq!(check_cell(&ex(), GOOD), Ok(4));
    }
    #[test]
    fn escapes_and_whitespace() {
        let c = "[ [\"\\u002f\" , 0,0, 1],[\"-\",0,1,2],\n[\"+\",0,2,3],[\"+\",0,3,4] ] ";
        assert_eq!(check_cell(&ex(), c), Ok(4));
    }
    #[test]
    fn rejects() {
        let p = ex();
        for c in [
            r#"[["+",-0,0,1]]"#,
            r#"[["+",0.0,0,1]]"#,
            r#"[["+",true,0,1]]"#,
            r#"[["+",0,0,48]]"#,
            r#"[["+",0,0,1,2]]"#,
            r#"[["x",0,0,1]]"#,
            r#"[["+",0,0,1]"#,
            r#"[["+",01,0,1]]"#,
            r#"{"a":1}"#,
            r#"[["-",0,0,1]]"#,
            r#"[["*",0,0,1],["+",1,0,2]]"#,
        ] {
            assert!(check_cell(&p, c).is_err(), "{c}");
        }
    }
    #[test]
    fn limit_edges() {
        let p = Puzzle { start: vec![1_000_000_000], targets: vec![1_000_000_000_000_000_000], limit: 1_000_000_000_000_000_000, registers: 48 };
        assert_eq!(check_cell(&p, r#"[["*",0,0,1]]"#), Ok(1));
        let q = Puzzle { limit: 999_999_999_999_999_999, ..p.clone() };
        assert!(check_cell(&q, r#"[["*",0,0,1]]"#).is_err());
        let big = Puzzle { start: vec![u64::MAX / 2 + 1], targets: vec![1], limit: u64::MAX, registers: 48 };
        assert!(check_cell(&big, r#"[["*",0,0,1]]"#).is_err()); // 2^126 > u64::MAX, no wrap
    }
    #[test]
    fn review_regressions() {
        let p = ex();
        // \u+XXX is not a JSON escape (u32::from_str_radix would accept the '+')
        assert!(check_cell(&p, r#"[["\u+02f",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#).is_err());
        assert_eq!(check_cell(&p, r#"[["\u002f",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4]]"#), Ok(4));
        // malformed puzzles are reported, never panic
        let zero = Puzzle { start: vec![0], targets: vec![1], limit: 9, registers: 48 };
        assert!(matches!(check_cell(&zero, r#"[["/",0,0,1]]"#), Err(Reject::InvalidPuzzle(_))));
        let crowded = Puzzle { start: vec![1, 2, 3], targets: vec![1], limit: 9, registers: 2 };
        assert!(matches!(check(&crowded, &[]), Err(Reject::InvalidPuzzle(_))));
        assert_eq!(apply(Op::Div, 4, 0, 9, 0), Err(Reject::DivisionByZero { step: 0 }));
        assert!(analyze(&zero, &[]).is_none());
        // strict CSV
        assert!(read_csv("id,program\n\"a\"b,1\n").is_none());
        assert!(read_csv("a,b\"c\n").is_none());
        assert!(read_csv("a\rb\n").is_none());
        assert_eq!(read_csv("\"\""), Some(vec![vec![String::new()]]));
        assert_eq!(read_csv("x,\"[[\"\"+\"\",0,0,1]]\"\r\n"), Some(vec![vec!["x".into(), "[[\"+\",0,0,1]]".into()]]));
        assert_eq!(parse_digits("+5"), None);
        assert_eq!(parse_digits("15"), Some(15));
    }

    #[test]
    fn dead_code() {
        let p = ex();
        let prog = parse_cell(r#"[["*",0,0,5],["/",0,0,1],["-",0,1,2],["+",0,2,3],["+",0,3,4],["+",0,0,6]]"#, 48).unwrap();
        let s = strip_dead(&p, &prog);
        assert_eq!(s.len(), 4);
        assert_eq!(to_json(&s), GOOD);
    }
}

// ---------------------------------------------------------------- C ABI (used by the WebAssembly playground)

/// Minimal C ABI so the library can run in a browser as WebAssembly without any bindings crate.
/// Enabled with the `ffi` feature (off by default, so Rust users get no global symbols).
///
/// Protocol: the host gets a buffer from `elf_alloc(len)`, writes UTF-8 into it, calls `elf_trace`, reads the JSON
/// result at `elf_result_ptr()` / `elf_result_len()`, then frees its buffers with `elf_free(ptr, len)` using the
/// same `len`. The result pointer stays valid only until the next `elf_trace` call on the same thread.
#[cfg(feature = "ffi")]
pub mod ffi {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout};
    use std::cell::RefCell;

    thread_local! { static RESULT: RefCell<String> = RefCell::new(String::new()); }

    fn layout(len: usize) -> Option<Layout> {
        Layout::from_size_align(len.max(1), 1).ok()
    }

    /// Allocate `len` bytes (alignment 1). Returns null if `len` is too large or allocation fails.
    #[no_mangle]
    pub extern "C" fn elf_alloc(len: usize) -> *mut u8 {
        match layout(len) {
            // SAFETY: the layout has non-zero size.
            Some(l) if len <= isize::MAX as usize => unsafe { alloc(l) },
            _ => std::ptr::null_mut(),
        }
    }

    /// Free a buffer from `elf_alloc(len)`; null is ignored.
    ///
    /// # Safety
    /// `ptr` must be null or come from `elf_alloc(len)` with the same `len`, and not have been freed.
    #[no_mangle]
    pub unsafe extern "C" fn elf_free(ptr: *mut u8, len: usize) {
        if ptr.is_null() {
            return;
        }
        if let Some(l) = layout(len) {
            dealloc(ptr, l);
        }
    }

    #[no_mangle]
    pub extern "C" fn elf_result_ptr() -> *const u8 {
        RESULT.with(|r| r.borrow().as_ptr())
    }

    #[no_mangle]
    pub extern "C" fn elf_result_len() -> usize {
        RESULT.with(|r| r.borrow().len())
    }

    fn esc(s: &str) -> String {
        let mut o = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o
    }

    fn failure(error: &str) -> String {
        format!("{{\"ok\":false,\"score\":0,\"error\":\"{}\",\"bad_step\":-1,\"steps\":[],\"uncovered\":[]}}", esc(error))
    }

    /// Trace a program. `puzzle` is "start;targets;limit;registers" with comma-separated decimal integers.
    /// Always returns the same JSON shape: {"ok","score","error","bad_step","steps":[{op,i,j,d,a,b,v,target,live}],
    /// "uncovered"}. Values are strings (they may exceed 2^53); `live` is null unless the whole program is legal.
    pub fn trace_json(puzzle: &str, cell: &str) -> String {
        let f: Vec<&str> = puzzle.split(';').collect();
        let nums = |s: &str| -> Option<Vec<u64>> {
            if s.trim().is_empty() { Some(vec![]) } else { s.split(',').map(parse_digits).collect() }
        };
        let parsed = (|| {
            if f.len() != 4 {
                return None;
            }
            let p = Puzzle { start: nums(f[0])?, targets: nums(f[1])?, limit: parse_digits(f[2])?, registers: parse_digits(f[3])? as usize };
            p.validate().ok()?;
            Some(p)
        })();
        let Some(p) = parsed else {
            return failure("puzzle must be start;targets;limit;registers with positive integers");
        };
        let prog = match parse_cell(cell, p.registers) {
            Ok(v) => v,
            Err(e) => return failure(&e.to_string()),
        };
        let mut regs: Vec<Option<u64>> = vec![None; p.registers];
        for (k, &v) in p.start.iter().enumerate() {
            regs[k] = Some(v);
        }
        let mut need: std::collections::BTreeSet<u64> = p.targets.iter().copied().collect();
        for v in &p.start {
            need.remove(v);
        }
        let live = analyze(&p, &prog);
        let mut steps = Vec::new();
        let mut err: Option<(usize, String)> = None;
        for (step, ins) in prog.iter().enumerate() {
            let (a, b) = (regs[ins.i], regs[ins.j]);
            let (Some(a), Some(b)) = (a, b) else {
                err = Some((step, Reject::EmptyRegister { step, reg: if a.is_none() { ins.i } else { ins.j } }.to_string()));
                break;
            };
            match apply(ins.op, a, b, p.limit, step) {
                Ok(v) => {
                    regs[ins.d] = Some(v);
                    let hit = need.remove(&v);
                    let l = match &live {
                        Some(x) => if x[step].live { "true" } else { "false" },
                        None => "null",
                    };
                    steps.push(format!(
                        "{{\"op\":\"{}\",\"i\":{},\"j\":{},\"d\":{},\"a\":\"{a}\",\"b\":\"{b}\",\"v\":\"{v}\",\"target\":{hit},\"live\":{l}}}",
                        ins.op.symbol(), ins.i, ins.j, ins.d
                    ));
                }
                Err(e) => {
                    err = Some((step, e.to_string()));
                    break;
                }
            }
        }
        let unc: Vec<String> = need.iter().map(|v| format!("\"{v}\"")).collect();
        let (ok, error, bad) = match err {
            Some((s, m)) => (false, m, s as i64),
            None if !need.is_empty() => (false, Reject::NotCovered(need.len()).to_string(), -1),
            None => (true, String::new(), -1),
        };
        format!(
            "{{\"ok\":{ok},\"score\":{},\"error\":\"{}\",\"bad_step\":{bad},\"steps\":[{}],\"uncovered\":[{}]}}",
            prog.len(), esc(&error), steps.join(","), unc.join(",")
        )
    }

    /// Read a host buffer as UTF-8. A zero length is the empty string whatever the pointer.
    unsafe fn host_str<'a>(ptr: *const u8, len: usize) -> Result<&'a str, &'static str> {
        if len == 0 {
            return Ok("");
        }
        if ptr.is_null() || len > isize::MAX as usize {
            return Err("invalid input buffer");
        }
        std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).map_err(|_| "input is not valid UTF-8")
    }

    /// # Safety
    /// Each pointer/length pair must be (null or any pointer, 0) or describe `len` readable bytes.
    #[no_mangle]
    pub unsafe extern "C" fn elf_trace(pp: *const u8, pl: usize, cp: *const u8, cl: usize) -> usize {
        let out = match (host_str(pp, pl), host_str(cp, cl)) {
            (Ok(puzzle), Ok(cell)) => trace_json(puzzle, cell),
            (Err(e), _) | (_, Err(e)) => failure(e),
        };
        RESULT.with(|r| *r.borrow_mut() = out);
        elf_result_len()
    }
}

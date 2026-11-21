// Elf Calculator replay benchmark in Goose.
// Input: whitespace-separated unsigned integers:
//   P, then per puzzle: ns, start[ns], nt, targets[nt], limit, ni, then ni x (op i j d), op 0..3 = + - * /
// Replays every program `reps` times and prints the total instruction count of the legal, complete ones.
// u64 arithmetic wraps in Goose, so every operation is checked before it is done:
// a + b <= 2e18 cannot wrap (values are <= limit <= 1e18), a * b is checked as a > limit / b.
import std;
import os;

fn parse(buf: u8[:], nums: u64[>..]&) {
    var v: u64 = 0;
    var in_num = false;
    for c in buf {
        if c >= 48 && c <= 57 {
            v = v * 10 + (c - 48);
            in_num = true;
        } else if in_num {
            nums.push(v);
            v = 0;
            in_num = false;
        }
    }
    if in_num { nums.push(v); }
}

fn check_all(nums: u64[:]) -> i64, i64, u64 {
    var pos = 0;
    var total = 0;
    var bad = 0;
    var sig: u64 = 0;            // verdict signature: sum of (p + 1) * score over legal programs, wrapping
    let np = nums[pos] as i64;
    pos++;
    for p in np {
        var regs: u64[48] = [0; 48];          // 0 = empty; legal values are >= 1
        let ns = nums[pos] as i64;
        pos++;
        for k in ns { regs[k] = nums[pos]; pos++; }
        let nt = nums[pos] as i64;
        pos++;
        let tpos = pos;
        pos += nt;
        let limit = nums[pos];
        pos++;
        let ni = nums[pos] as i64;
        pos++;
        var cov: u64 = 0;
        for t in nt {
            for k in ns { if regs[k] == nums[tpos + t] { cov |= (1 as u64) << t; } }
        }
        var ok = true;
        for s in ni {
            let op = nums[pos];
            let i = nums[pos + 1] as i64;
            let j = nums[pos + 2] as i64;
            let d = nums[pos + 3] as i64;
            pos += 4;
            if !ok { continue; }
            let a = regs[i];
            let b = regs[j];
            if a == 0 || b == 0 { ok = false; continue; }
            var r: u64 = 0;
            if op == 0 {
                r = a + b;
            } else if op == 1 {
                if a <= b { ok = false; continue; }
                r = a - b;
            } else if op == 2 {
                if a > limit / b { ok = false; continue; }
                r = a * b;
            } else {
                if a % b != 0 { ok = false; continue; }
                r = a / b;
            }
            if r > limit { ok = false; continue; }
            regs[d] = r;
            for t in nt { if nums[tpos + t] == r { cov |= (1 as u64) << t; } }
        }
        let full: u64 = ((1 as u64) << nt) - 1;
        if ok && cov == full { total += ni; sig += (p as u64 + 1) * (ni as u64); } else { bad++; }
    }
    return total, bad, sig;
}

fn main() {
    let a = args();
    guard a.len >= 2 else { print("usage: replay FILE [reps]"); return; }
    var buf: u8[>..] = [];
    guard read_file(a[1], buf) else { print("cannot read ", a[1]); return; }
    var reps = 20;
    if a.len >= 3 {
        let r, ok = parse_int(a[2]);
        if ok { reps = r; }
    }
    var nums: u64[>..] = [];
    let t0 = clock();
    parse(buf, nums);
    let t1 = clock();
    var total = 0;
    var bad = 0;
    var sig: u64 = 0;
    for r in reps { total, bad, sig = check_all(nums); }
    let t2 = clock();
    print("goose: score=", total, " bad=", bad, " sig=", sig, " parse ", (t1 - t0) * 1000.0, " ms, check ",
          (t2 - t1) * 1000.0 / reps, " ms/file");
}

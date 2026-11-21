#!/usr/bin/env python3
"""Like-for-like replay benchmark: Python vs Rust vs Goose, same algorithm and same numeric input.
Writes two inputs to --dir, derived from your own submission file:
  bank_nums.txt  the submission (all legal),
  fuzz_nums.txt  each bank program plus K mutated copies (illegal or incomplete ones included).
Every implementation must report the same (score, bad, signature) as check_program from
santa2026_verify.py (https://github.com/Chih-Kai-Wang/santa-2026-submission-verifier).
Usage: three_lang_bench.py --puzzles puzzles.csv --sub submission.csv --verifier santa2026_verify.py
                           --dir D --rust BIN --goose BIN [--reps 20] [--fuzz 4]"""
import argparse, csv, json, os, random, subprocess, sys, time
import importlib.util
csv.field_size_limit(1 << 30)
OP = {"+": 0, "-": 1, "*": 2, "/": 3}
MASK = (1 << 64) - 1


def encode(cases):
    out = [str(len(cases))]
    for p, prog in cases:
        t = sorted(set(p["targets"]))
        out += [str(len(p["start"]))] + list(map(str, p["start"])) + [str(len(t))] + list(map(str, t))
        out += [str(p["limit"]), str(len(prog))]
        for op, i, j, d in prog:
            out += [str(OP[op]), str(i), str(j), str(d)]
    return " ".join(out) + "\n"


def expected(cases):
    total = bad = sig = 0
    for k, (p, prog) in enumerate(cases):
        if check_program(p, prog)[0]:
            total += len(prog); sig = (sig + (k + 1) * len(prog)) & MASK
        else:
            bad += 1
    return total, bad, sig


def py_flat(nums):
    """The same flat algorithm in plain Python."""
    pos = 0; total = bad = sig = 0
    np_ = nums[pos]; pos += 1
    for p in range(np_):
        regs = [0] * 48
        ns = nums[pos]; pos += 1
        regs[:ns] = nums[pos:pos + ns]; pos += ns
        nt = nums[pos]; pos += 1
        tg = nums[pos:pos + nt]; pos += nt
        limit, ni = nums[pos], nums[pos + 1]; pos += 2
        cov = 0
        for t, v in enumerate(tg):
            if v in regs[:ns]: cov |= 1 << t
        ok = True
        for _ in range(ni):
            op, i, j, d = nums[pos], nums[pos + 1], nums[pos + 2], nums[pos + 3]; pos += 4
            if not ok: continue
            a, b = regs[i], regs[j]
            if a == 0 or b == 0: ok = False; continue
            if op == 0: r = a + b
            elif op == 1:
                if a <= b: ok = False; continue
                r = a - b
            elif op == 2:
                if a > limit // b: ok = False; continue
                r = a * b
            else:
                if a % b: ok = False; continue
                r = a // b
            if r > limit: ok = False; continue
            regs[d] = r
            for t, v in enumerate(tg):
                if v == r: cov |= 1 << t
        if ok and cov == (1 << nt) - 1:
            total += ni; sig = (sig + (p + 1) * ni) & MASK
        else:
            bad += 1
    return total, bad, sig


def mutate(rng, prog):
    q = [list(x) for x in prog]
    if not q: return [["+", 0, 0, 1]]
    k = rng.randrange(len(q)); m = rng.randrange(5)
    if m == 0: q[k][0] = rng.choice("+-*/")
    elif m == 1: q[k][rng.randrange(1, 4)] = rng.randrange(48)
    elif m == 2: del q[k]
    elif m == 3: q.insert(k, [rng.choice("+-*/"), rng.randrange(48), rng.randrange(48), rng.randrange(48)])
    else: q[k], q[-1] = q[-1], q[k]
    return q


def parse_out(line):
    f = dict(x.split("=") for x in line.split() if "=" in x)
    return int(f["score"]), int(f["bad"]), int(f["sig"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--puzzles", required=True); ap.add_argument("--sub", required=True)
    ap.add_argument("--verifier", required=True)
    ap.add_argument("--dir", required=True); ap.add_argument("--rust", required=True); ap.add_argument("--goose", required=True)
    ap.add_argument("--reps", type=int, default=20); ap.add_argument("--fuzz", type=int, default=4)
    a = ap.parse_args()
    spec = importlib.util.spec_from_file_location("santa2026_verify", a.verifier)
    V = importlib.util.module_from_spec(spec); spec.loader.exec_module(V)
    global check_program
    check_program = V.check_program
    P = V.load_puzzles(a.puzzles)
    B = {r[0]: json.loads(r[1]) for r in list(csv.reader(open(a.sub, newline="")))[1:]}
    rng = random.Random(5)
    bank = [(P[k], B[k]) for k in sorted(B)]
    fuzz = []
    for p, prog in bank:
        fuzz.append((p, prog))
        fuzz += [(p, mutate(rng, prog)) for _ in range(a.fuzz)]
    fails = 0
    for name, cases in (("bank", bank), ("fuzz", fuzz)):
        path = os.path.join(a.dir, f"{name}_nums.txt")
        open(path, "w").write(encode(cases))
        exp = expected(cases)
        nums = [int(x) for x in open(path).read().split()]
        reps = a.reps if name == "bank" else 1
        t = time.perf_counter()
        for _ in range(reps): got_py = py_flat(nums)
        tpy = (time.perf_counter() - t) / reps * 1e3
        t = time.perf_counter()
        for _ in range(reps): [check_program(p, prog) for p, prog in cases]
        tgate = (time.perf_counter() - t) / reps * 1e3
        rs = subprocess.run([a.rust, path, str(reps)], capture_output=True, text=True, check=True).stdout.strip()
        go = subprocess.run([a.goose, path, str(reps)], capture_output=True, text=True, check=True).stdout.strip()
        got = {"python": got_py, "rust": parse_out(rs), "goose": parse_out(go)}
        agree = all(v == exp for v in got.values())
        fails += not agree
        print(f"[{name}] cases={len(cases)} expected (score,bad,sig)={exp} -> {'ALL AGREE' if agree else got}")
        print(f"   python gate check_program {tgate:.1f} ms | python flat {tpy:.1f} ms/file")
        print("  ", rs); print("  ", go)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()

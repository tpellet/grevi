# /// script
# requires-python = ">=3.11"
# ///
"""Route each eval intent with `jevify run --json --dry-run --no-args`; compare to BM25 over the same
frozen inventory; print a reliability table for `fit`."""
import json
import math
import os
import re
import subprocess
import sys
from pathlib import Path

H = "./target/release/jevify"  # fixed: ubs's taint check rejects an argv-selected executable
INV = "evals/inventory.json"
os.environ["JEVIFY_INVENTORY_FILE"] = INV
# jevify and BM25 rank the same frozen tool list, so the comparison is like for like and reproducible.
docs = {t["name"]: (t["name"] + " " + t["summary"]).lower() for t in json.loads(Path(INV).read_text())}
def tok(s): return re.findall(r"[a-z0-9]+", s.lower())
N = len(docs); avg = sum(len(tok(d)) for d in docs.values()) / max(N, 1)
df = {}
for d in docs.values():
    for w in set(tok(d)): df[w] = df.get(w, 0) + 1
def bm25(q):
    best = (0.0, None)
    for n, d in docs.items():
        dt = tok(d); s = 0.0
        for w in tok(q):
            tf = dt.count(w)
            if tf: s += math.log(1 + (N - df[w] + .5) / (df[w] + .5)) * tf * 2.2 / (tf + 1.2 * (.25 + .75 * len(dt) / avg))
        if s > best[0]: best = (s, n)
    return best[1]
errors = 0
near = 0  # routes whose fit sits within the measured run-to-run jitter (0.06) of the threshold
def route(request, *opts):
    global errors, near
    out = subprocess.run([H, "--json", "run", "--dry-run", *opts, request], capture_output=True, text=True, check=False)
    # A panic or a killed process leaves no envelope at all: count it as an error row rather than
    # crash and lose a paid run of up to ~170 requests. Error envelopes (exit 4/5/6: rate limit
    # exhausted, expired key, 413/422) do arrive, with `data: null`; count and report those too.
    try:
        v = json.loads(out.stdout)
    except json.JSONDecodeError:
        errors += 1; print(f"no envelope (exit {out.returncode}) for {request!r}: {out.stderr.strip()[:200]}", file=sys.stderr)
        return None, 0.0, 0.0, out.returncode, []
    d = v.get("data") or {}
    if not v["ok"]:
        errors += 1; print(f"error exit {v['exit_code']}: {v['error']['kind']} for {request!r}", file=sys.stderr)
    fit = d.get("fit", 0.0)
    # Identical requests differ by up to 0.06 without the cache: a route this close to the
    # threshold can flip on a re-run. Counted, not hidden.
    if "fit" in d and abs(fit - v["meta"]["threshold"]) < 0.06: near += 1
    return d.get("tool"), fit, v["meta"]["cost_usd"], v["exit_code"], d.get("argv") or []
bins = {}  # fit bin -> [n, correct]
def reliability(fit, correct):
    b = min(int(fit * 5), 4); bins.setdefault(b, [0, 0]); bins[b][0] += 1; bins[b][1] += int(correct)
Path("evals/out").mkdir(exist_ok=True)
# Rows are written after every request, so an interrupted run keeps what it paid for.
def save(name, rows): Path("evals/out", name).write_text(json.dumps(rows, indent=1))
# 1. hand-written intents (49, by the author): top-1, abstention, BM25 baseline
cases = json.loads(Path("evals/run_intents.json").read_text())
hit = bm = abst_ok = abst_n = rout_n = 0; cost = 0.0; rows = []
for c in cases:
    tool, fit, cst, code, _ = route(c["request"], "--no-args"); cost += cst
    if c["ok"]:
        rout_n += 1; hit += tool in c["ok"]; bm += bm25(c["request"]) in c["ok"]
        # Same rule as set 2: the table covers routes jevify acted on. An abstention (exit 3) carries
        # no tool and an error row carries fit 0.0; both would land in the low bins as failures.
        if tool is not None and code == 0: reliability(fit, tool in c["ok"])
    else:
        abst_n += 1; abst_ok += code == 3  # an error envelope is not a correct abstention
    rows.append({"request": c["request"], "tool": tool, "fit": fit, "ok": c["ok"], "exit": code}); save("run.json", rows)
print(f"hand-written: routable top-1 {hit}/{rout_n}  BM25 {bm}/{rout_n}  abstain correct {abst_ok}/{abst_n}  errors {errors}  cost ${cost:.4f}")
# 2. NL2Bash held-out sample (120, not written by the author): gold = head utility
held = json.loads(Path("evals/nl2bash_sample.json").read_text())
h1 = hb = abst = 0; hrows = []; errors_before = errors
for c in held:
    tool, fit, cst, code, _ = route(c["request"], "--no-args"); cost += cst
    ok = tool == c["head"]; h1 += ok; hb += bm25(c["request"]) == c["head"]; abst += code == 3
    if tool is not None and code == 0: reliability(fit, ok)
    hrows.append({"request": c["request"], "tool": tool, "fit": fit, "gold": c["head"], "leaks_name": c.get("leaks_name"), "exit": code}); save("nl2bash.json", hrows)
print(f"nl2bash held-out: top-1 {h1}/{len(held)}  BM25 {hb}/{len(held)}  abstained {abst}/{len(held)}  errors {errors - errors_before}  total cost ${cost:.4f}")
print("reliability of routes jevify acted on, both sets, fit >= threshold (fit bin: n, accuracy):", {f"{b/5:.1f}-{(b+1)/5:.1f}": (n, round(k / n, 2)) for b, (n, k) in sorted(bins.items())})
print(f"near-threshold routes, both sets (|fit - threshold| < 0.06, the measured run-to-run jitter; a --no-cache re-run can flip them): {near}")
# 3. argument-pointing spot check (20, by the author), without --no-args: tool right; every flag a correct
# command must carry present (counted only when the tool is right; each entry of `required` lists the
# spellings that satisfy it); any flag beyond `required` + `optional` (only when the tool is right).
def flags(argv):
    # `-xzf` -> -x -z -f; `-n50`, `-iTCP:8080`, `-5` -> -n, -i, -5; `--max-depth=1` -> --max-depth; values skipped.
    out = set()
    for a in argv[1:]:
        if a == "--": break  # everything after it is an operand
        if a.startswith("--"): out.add(a.split("=", 1)[0])
        elif a.startswith("-") and a[1:].isalpha(): out.update("-" + ch for ch in a[1:])
        elif len(a) > 1 and a.startswith("-"): out.add(a[:2])
    return out
acases = json.loads(Path("evals/run_args.json").read_text())
tool_ok = flags_ok = extra_n = 0; arows = []; errors_before = errors; cost_before = cost
for c in acases:
    tool, fit, cst, code, argv = route(c["request"]); cost += cst
    got = flags(argv); right = tool == c["tool"]; tool_ok += right
    missing = [g[0] for g in c["required"] if not any(f in got for f in g)] if right else [g[0] for g in c["required"]]
    flags_ok += right and not missing
    extra = sorted(got - {f for g in c["required"] for f in g} - set(c["optional"])) if right else []
    extra_n += bool(extra)
    arows.append({"request": c["request"], "tool": tool, "expect": c["tool"], "argv": argv, "fit": fit, "missing": missing, "extra": extra, "exit": code}); save("args.json", arows)
print(f"args spot check: tool right {tool_ok}/{len(acases)}  all required flags present {flags_ok}/{len(acases)}  any extra flag {extra_n}/{len(acases)}  errors {errors - errors_before}  cost ${cost - cost_before:.4f}")

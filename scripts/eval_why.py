# /// script
# requires-python = ">=3.11"
# ///
"""Point at the root cause of every `evals/why/<id>.log` with `hunch why --json -n 3`; compare hit@1
and hit@3 with two free regex baselines on the same files; print where found cases' `any` sits."""
import json
import re
import statistics
import subprocess
import sys
from pathlib import Path

H = "./target/release/hunch"  # fixed: ubs's taint check rejects an argv-selected executable
# Copied from src/cmd/why.rs (SIGNAL); the baselines use exactly what hunch's prefilter uses.
SIGNAL = re.compile(r"(?i)\b(error|err!|fail(ed|ure|s)?|fatal|panic(ked)?|exception|traceback|denied|not found|no such|cannot|can't|couldn't|undefined|unresolved|refused|timed? ?out|segmentation|abort(ed)?|killed|exit (code|status) [1-9]|assert)")
cases = sorted(Path("evals/why").glob("*.log"))
hit = {"hunch": [0, 0], "first SIGNAL match": [0, 0], "last SIGNAL match": [0, 0]}  # name -> [hit@1, hit@3]
errors = 0; abstained = 0; anys = []; cost = 0.0; rows = []
Path("evals/out").mkdir(exist_ok=True)
def score(name, pointed, lo, hi):
    # `pointed` is ranked, 1-based; a hit is a pointed line inside the labelled root-cause range.
    hit[name][0] += bool(pointed) and lo <= pointed[0] <= hi
    hit[name][1] += any(lo <= p <= hi for p in pointed[:3])
for log in cases:
    exp = json.loads(log.with_suffix(".expect").read_text())
    lo, hi = exp["lines"]
    text = log.read_text()
    # Same line numbering as hunch (Rust `str::lines`): split on "\n", no empty line after a final newline.
    lines = text.split("\n")
    if lines and lines[-1] == "": lines.pop()
    sig = [i + 1 for i, l in enumerate(lines) if SIGNAL.search(l)]
    score("first SIGNAL match", sig, lo, hi); score("last SIGNAL match", sig[::-1], lo, hi)
    out = subprocess.run([H, "--json", "why", "-n", "3"], input=text, capture_output=True, text=True, check=False)
    # No envelope (a panic, a killed process) is an error row, never a crash: the run keeps what it paid for.
    try:
        v = json.loads(out.stdout)
    except json.JSONDecodeError:
        errors += 1; print(f"no envelope (exit {out.returncode}) for {log.name}: {out.stderr.strip()[:200]}", file=sys.stderr)
        rows.append({"case": log.stem, "pointed": [], "any": None, "expect": [lo, hi], "exit": out.returncode}); continue
    d = v.get("data") or {}
    # Exit 3 is an honest "no failure found": a miss for the table, not an error.
    if not v["ok"] and v["exit_code"] != 3:
        errors += 1; print(f"error exit {v['exit_code']}: {v['error']['kind']} for {log.name}", file=sys.stderr)
    abstained += v["exit_code"] == 3
    pointed = [c["line"] for c in d.get("causes") or []]
    score("hunch", pointed, lo, hi)
    if "any" in d and d["any"] is not None: anys.append(d["any"])
    cost += v["meta"]["cost_usd"]
    rows.append({"case": log.stem, "pointed": pointed, "any": d.get("any"), "expect": [lo, hi], "exit": v["exit_code"]})
    Path("evals/out/why.json").write_text(json.dumps(rows, indent=1))
n = len(cases)
print(f"why: {n} cases  abstained {abstained}  errors {errors}  cost ${cost:.4f}")
print(f"{'method':<20} {'hit@1':>8} {'hit@3':>8}")
for name, (h1, h3) in hit.items():
    print(f"{name:<20} {h1:>5}/{n:<3}{h3:>5}/{n:<3}")
if anys:
    # Every case holds a failure, so this is where found cases' absolute Noul sits against the 0.5 knob.
    print(f"data.any over {len(anys)} answered cases: min {min(anys):.2f}  median {statistics.median(anys):.2f}  (threshold {v['meta']['threshold']})")

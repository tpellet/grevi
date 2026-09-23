#!/usr/bin/env python3
"""Score a run of this set: coverage, accuracy, repeated wrong answers, and
whether the right target reached the finals of round one.

Usage: score.py <run.jsonl> [<cases.jsonl>]
"""
import collections
import json
import pathlib
import sys

run = [json.loads(l) for l in pathlib.Path(sys.argv[1]).read_text().splitlines() if l.strip()]
cases = {}
if len(sys.argv) > 2:
    for l in pathlib.Path(sys.argv[2]).read_text().splitlines():
        if l.strip():
            c = json.loads(l)
            cases[c["id"]] = c

decided = [r for r in run if r.get("answer")]
right = [r for r in decided if r["correct"]]
wrong = [r for r in decided if not r["correct"]]
abst = [r for r in run if not r.get("answer")]

print(f"backend {run[0]['backend']}  cases {len(run)}")
print(f"  decided {len(decided)}  right {len(right)}  wrong {len(wrong)}  abstained {len(abst)}")
print(f"  accuracy on decided {len(right)}/{len(decided)}"
      f"  hit@1 over all {len(right)}/{len(run)}")
by_reason = collections.Counter(r.get("reason") for r in abst)
print("  abstention reasons:", dict(by_reason))

rep = collections.Counter(r.get("answer_short") or r.get("answer") for r in wrong)
print("  wrong answers repeated:", {k: v for k, v in rep.items() if v > 1} or "none")
print("  every wrong answer:", dict(rep))
print("  332191a returned for:",
      [r["id"] for r in decided if (r.get("answer_short") or "") == "332191a"])

# finalists: was the target even in the finals of round one?
inf = outf = unknown = 0
missing = []
for r in run:
    fin = r.get("finalists")
    if fin is None:
        unknown += 1
        continue
    if r["target"] in fin:
        inf += 1
    else:
        outf += 1
        missing.append(r["id"])
print(f"  target in the finals {inf}, not in the finals {outf}, no finals recorded {unknown}")
print("  target missing from the finals:", missing)

# failures by window: which window holds the target
w = collections.Counter()
for r in run:
    c = cases.get(r["id"])
    if not c or "index" not in c or not r.get("windows"):
        continue
    per = 99 if run[0]["backend"] == "classifier" else 200
    win = (c["index"] - 1) // per + 1
    w[(win, "right" if r["correct"] else "miss")] += 1
print("  by window of the target (window, outcome) -> n:", dict(sorted(w.items())))
print("  windows seen:", collections.Counter(r.get("windows") for r in run))
print("  requests per case:", collections.Counter(r.get("requests") for r in run))

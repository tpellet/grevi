#!/usr/bin/env python3
"""Score one or more runs of this set.

Usage: score.py <run.jsonl> [<run.jsonl> ...]
"""
import collections
import json
import pathlib
import sys

for path in sys.argv[1:]:
    run = [json.loads(l) for l in pathlib.Path(path).read_text().splitlines() if l.strip()]
    if not run:
        continue
    decided = [r for r in run if r.get("answer")]
    right = [r for r in decided if r["correct"]]
    wrong = [r for r in decided if not r["correct"]]
    claim = [r for r in decided if r.get("chose_claimant")]
    seconds = sorted(r.get("seconds", 0) for r in run)
    print(f"{path}  backend {run[0]['backend']}  binary {run[0].get('binary')}  cases {len(run)}")
    print(f"  decided {len(decided)}  right {len(right)}  wrong {len(wrong)}"
          f"  abstained {len(run) - len(decided)}")
    print(f"  hit@1 {len(right)}/{len(run)}   chose the lying subject {len(claim)}"
          f" ({[r['id'] for r in claim]})")
    print("  abstention reasons:",
          dict(collections.Counter(r.get("reason") for r in run if not r.get("answer"))))
    print("  confidence on wrong answers:", [r.get("p") for r in wrong])
    inf = [r for r in run if r.get("finalists") and r["target"] in r["finalists"]]
    known = [r for r in run if r.get("finalists") is not None]
    print(f"  target reached the finals {len(inf)}/{len(known)}")
    print("  requests per case:", dict(collections.Counter(r.get("requests") for r in run)))
    print(f"  seconds per case: median {seconds[len(seconds) // 2]}  total {round(sum(seconds), 1)}")

#!/usr/bin/env python3
"""Does the pool size alone explain the misses?

The same descriptions choose over the same evidence (one `sha\\tsubject` line per
commit, the `-` kind, no tier-two round) from a pool of 60 and a pool of 248.
Only the cases whose target is among the newest 60 commits can run, so both pools
contain the answer. Writes runs/pool-probe.jsonl.
"""
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/jevify"
HERE = ROOT / "evals/commit-attractor"

cases = [json.loads(l) for l in (HERE / "cases.jsonl").read_text().splitlines() if l.strip()]
cases = [c for c in cases if c["index"] <= 60]

rows = []
for backend in ("classifier", "typesafe"):
    env = dict(os.environ, JEVIFY_NO_CACHE="1", JEVIFY_BACKEND=backend)
    if backend == "typesafe":
        env["TYPESAFE_API_KEY_FILE"] = os.path.expanduser("~/.ssh/typesafe-ai-key")
    for pool in (60, 248):
        for c in cases:
            proc = subprocess.run(
                [str(BIN), "fill", "--dry-run", "--json", "--candidates",
                 str(HERE / f"pools/subjects-{pool}.tsv"), "--field", "1", "--",
                 "git", "show", "@{-:%s}" % c["description"]],
                capture_output=True, text=True, env=env, cwd=str(ROOT),
            )
            e = json.loads(proc.stdout)
            m = e["data"]["markers"][0]
            row = {
                "id": c["id"], "backend": backend, "pool": pool, "target": c["target"],
                "answer": (m.get("handle") or "")[:7] or None, "reason": m.get("reason"),
                "p": m.get("p"), "candidates": m.get("candidates"),
                "gates": e["meta"]["decision"].get("gates"),
            }
            row["correct"] = row["answer"] == c["target"]
            rows.append(row)
            print(backend, pool, row["id"], row["reason"], row["answer"], row["p"],
                  "correct" if row["correct"] else "", flush=True)

(HERE / "runs/pool-probe.jsonl").write_text("\n".join(json.dumps(r) for r in rows) + "\n")
for backend in ("classifier", "typesafe"):
    for pool in (60, 248):
        sub = [r for r in rows if r["backend"] == backend and r["pool"] == pool]
        print(backend, pool, "right", sum(r["correct"] for r in sub), "of", len(sub))

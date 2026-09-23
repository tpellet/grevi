#!/usr/bin/env python3
"""Run one case file of this set against one repository on one backend.

Usage: run.py <classifier|typesafe> <cases.jsonl> <repo> <out.jsonl>

The binary comes from JEVIFY_BIN, so the same cases run against two builds.
Every run sets JEVIFY_NO_CACHE=1 and JEVIFY_DECISION=round_one, and uses
`fill --dry-run`, so no answer is reused and no filled command is executed.
"""
import json
import os
import pathlib
import subprocess
import sys
import time

backend, cases_path, repo, out_path = sys.argv[1:5]
repo = str(pathlib.Path(repo).resolve())
binary = os.environ.get(
    "JEVIFY_BIN",
    str(pathlib.Path(__file__).resolve().parents[2] / "target/debug/jevify"),
)
shas = subprocess.run(
    ["git", "-C", repo, "rev-list", "HEAD"],
    capture_output=True, text=True, check=True,
).stdout.split()

env = dict(os.environ)
env["JEVIFY_NO_CACHE"] = "1"
env["JEVIFY_DECISION"] = "round_one"
env["JEVIFY_BACKEND"] = backend
if backend == "typesafe":
    env["TYPESAFE_API_KEY_FILE"] = os.path.expanduser("~/.ssh/typesafe-ai-key")

out = []
for line in pathlib.Path(cases_path).read_text().splitlines():
    if not line.strip():
        continue
    case = json.loads(line)
    marker = "@{%s:%s}" % (case["kind"], case["description"])
    argv = [binary, "fill", "--dry-run", "--json", "--", "git", "show", marker]
    started = time.monotonic()
    proc = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=repo)
    row = {"id": case["id"], "backend": backend, "binary": pathlib.Path(binary).name,
           "target": case["target"], "claimant": case.get("claimant"),
           "exit_code": proc.returncode, "seconds": round(time.monotonic() - started, 2)}
    try:
        envelope = json.loads(proc.stdout)
    except json.JSONDecodeError:
        row["error"] = proc.stdout[-400:] + proc.stderr[-400:]
        out.append(row)
        print(row["id"], "PARSE FAIL", flush=True)
        continue
    m = envelope["data"]["markers"][0]
    dec = envelope["meta"]["decision"]
    row.update(
        answer=(m.get("handle") or "")[:7] or None,
        reason=m.get("reason"),
        p=m.get("p"),
        candidates=m.get("candidates"),
        total=m.get("total"),
        model=envelope["meta"]["model"],
        requests=envelope["meta"]["requests"],
    )
    ro = (dec.get("round_one") or [None])[0]
    if ro:
        row["windows"] = len(ro["windows"])
        row["finalists"] = [shas[i - 1][:7] for i in ro["finalists"]]
    row["correct"] = row.get("answer") == case["target"]
    row["chose_claimant"] = bool(case.get("claimant")) and row.get("answer") == case["claimant"]
    out.append(row)
    print(row["id"], row.get("reason"), row.get("answer"), row.get("p"),
          "CORRECT" if row["correct"] else ("claimant" if row["chose_claimant"] else ""),
          f'{row["seconds"]}s', flush=True)

pathlib.Path(out_path).write_text("\n".join(json.dumps(r) for r in out) + "\n")
print("wrote", out_path, len(out), "rows")

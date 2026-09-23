#!/usr/bin/env python3
"""Run every case of this set on one backend and write one JSON line per case.

Usage: run.py <classifier|typesafe> <cases.jsonl> <out.jsonl>

Every run sets JEVIFY_NO_CACHE=1 and JEVIFY_DECISION=round_one, so no answer is
reused and the finalists of round one are recorded next to the decision.
`fill --dry-run` never executes the command it fills.
"""
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/jevify"

backend, cases_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
shas = subprocess.run(
    ["git", "-C", str(ROOT), "rev-list", "HEAD"],
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
    argv = [str(BIN), "fill", "--dry-run", "--json", "--"]
    argv += ["git", "show", marker] if case["kind"] == "commit" else ["git", "switch", marker]
    proc = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=str(ROOT))
    row = {"id": case["id"], "backend": backend, "target": case["target"],
           "exit_code": proc.returncode}
    try:
        env_json = json.loads(proc.stdout)
    except json.JSONDecodeError:
        row["error"] = proc.stdout[-400:] + proc.stderr[-400:]
        out.append(row)
        print(row["id"], "PARSE FAIL", flush=True)
        continue
    m = env_json["data"]["markers"][0]
    dec = env_json["meta"]["decision"]
    row.update(
        answer=m.get("handle"),
        reason=m.get("reason"),
        p=m.get("p"),
        candidates=m.get("candidates"),
        total=m.get("total"),
        model=env_json["meta"]["model"],
        gates=dec.get("gates"),
        requests=env_json["meta"]["requests"],
    )
    ro = (dec.get("round_one") or [None])[0]
    if ro:
        row["windows"] = len(ro["windows"])
        row["finalist_indices"] = ro["finalists"]
        if case["kind"] == "commit":
            row["finalists"] = [shas[i - 1][:7] for i in ro["finalists"]]
            row["round_one_tops"] = [
                [(shas[r["index"] - 1][:7], r["p"]) for r in w["ranks"][:3]]
                for w in ro["windows"]
            ]
    if row.get("answer") and case["kind"] == "commit":
        row["answer_short"] = row["answer"][:7]
    row["correct"] = (
        (row.get("answer_short") or row.get("answer")) == case["target"]
        if row.get("answer")
        else False
    )
    out.append(row)
    print(row["id"], row.get("reason"), row.get("answer_short") or row.get("answer"),
          row.get("p"), "correct" if row["correct"] else "", flush=True)

pathlib.Path(out_path).write_text("\n".join(json.dumps(r) for r in out) + "\n")
print("wrote", out_path, len(out), "rows")

#!/usr/bin/env python3
"""Candidate phrasings for the nothing-fits demonstration.

Each phrasing runs the `commit` kind over this repository's whole history, four
times on each backend, with the answer cache off. One JSON line per run records
the decision reason and the three gate numbers the decision reads: the best
candidate, the next candidate and NONE.

Usage: run.py [out.jsonl]
"""
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
BIN = ROOT / "target/debug/jevify"
HERE = pathlib.Path(__file__).resolve().parent
REPEATS = 4

PHRASES = [
    ("go", "rewrote everything in Go"),
    ("android", "ports the user interface to Android"),
    ("oracle", "migrates the database from Oracle to Postgres"),
    ("k8s", "adds Kubernetes operator manifests for the staging cluster"),
    ("japanese", "translates the user manual into Japanese"),
    ("decoder", "fixes the memory leak in the video decoder"),
    ("billing", "adds billing and subscription management to the web dashboard"),
    ("ios", "ships the iOS app to the App Store"),
]

shas = subprocess.run(
    ["git", "-C", str(ROOT), "rev-list", "HEAD"],
    capture_output=True, text=True, check=True,
).stdout.split()

rows = []
for backend in ("classifier", "typesafe"):
    env = dict(os.environ, JEVIFY_NO_CACHE="1", JEVIFY_BACKEND=backend)
    if backend == "typesafe":
        env["TYPESAFE_API_KEY_FILE"] = os.path.expanduser("~/.ssh/typesafe-ai-key")
    for pid, desc in PHRASES:
        for rep in range(1, REPEATS + 1):
            proc = subprocess.run(
                [str(BIN), "fill", "--dry-run", "--json", "--", "git", "show",
                 "@{commit:%s}" % desc],
                capture_output=True, text=True, env=env, cwd=str(ROOT),
            )
            row = {"phrase": pid, "description": desc, "backend": backend,
                   "repeat": rep, "exit_code": proc.returncode}
            try:
                e = json.loads(proc.stdout)
            except json.JSONDecodeError:
                row["error"] = (proc.stdout[-300:] + proc.stderr[-300:])
                rows.append(row)
                print(backend, pid, rep, "PARSE FAIL", flush=True)
                continue
            m = e["data"]["markers"][0]
            g = (e["meta"]["decision"].get("gates") or [{}])[-1]
            row.update(
                reason=m.get("reason"),
                answer=(m.get("handle") or "")[:7] or None,
                p=m.get("p"),
                best=g.get("best"), next=g.get("next"),
                none=g.get("none"), any=g.get("any"),
                total=m.get("total"),
                model=e["meta"]["model"],
            )
            if row["answer"]:
                row["subject"] = subprocess.run(
                    ["git", "-C", str(ROOT), "show", "-s", "--format=%s", row["answer"]],
                    capture_output=True, text=True).stdout.strip()
            rows.append(row)
            print(backend, pid, rep, row["reason"], row["answer"],
                  "best", row["best"], "next", row["next"], "none", row["none"],
                  flush=True)

out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "runs/phrases.jsonl"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("\n".join(json.dumps(r) for r in rows) + "\n")

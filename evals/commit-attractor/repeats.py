#!/usr/bin/env python3
"""Repeats and negative controls.

Three descriptions run three times each on both backends, to show how stable one
answer is; plus the two descriptions that fit no commit in this history, which
must return no_match rather than any commit. Writes runs/repeats.jsonl.
"""
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/jevify"
HERE = ROOT / "evals/commit-attractor"

REPEATS = [
    ("commit-037", "332191a",
     "gives the stdin picker a second round over its finalists with its own prompt, so a "
     "description of behaviour lands on the implementation rather than on the page that "
     "documents it"),
    ("commit-225", "be994f2", "implements the blake3-keyed disk cache with a seven-day expiry"),
    ("commit-085", "7bcf70c",
     "caps the keyless batch at sixty records because the service refuses seventy-five with a "
     "spending limit, and reports that refusal as unavailable instead of a protocol error"),
]
NEGATIVE = [
    ("neg-go", None, "rewrote everything in Go"),
    ("neg-android", None, "ports the user interface to Android"),
]

rows = []
for backend in ("classifier", "typesafe"):
    env = dict(os.environ, JEVIFY_NO_CACHE="1", JEVIFY_BACKEND=backend)
    if backend == "typesafe":
        env["TYPESAFE_API_KEY_FILE"] = os.path.expanduser("~/.ssh/typesafe-ai-key")
    for cid, target, desc in REPEATS * 3 + NEGATIVE:
        proc = subprocess.run(
            [str(BIN), "fill", "--dry-run", "--json", "--", "git", "show",
             "@{commit:%s}" % desc],
            capture_output=True, text=True, env=env, cwd=str(ROOT),
        )
        e = json.loads(proc.stdout)
        m = e["data"]["markers"][0]
        row = {"id": cid, "backend": backend, "target": target,
               "answer": (m.get("handle") or "")[:7] or None,
               "reason": m.get("reason"), "p": m.get("p"),
               "gates": e["meta"]["decision"].get("gates")}
        rows.append(row)
        print(backend, cid, row["reason"], row["answer"], row["p"], flush=True)

(HERE / "runs/repeats.jsonl").write_text("\n".join(json.dumps(r) for r in rows) + "\n")

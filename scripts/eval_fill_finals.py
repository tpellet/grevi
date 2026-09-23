#!/usr/bin/env python3
"""Run the held-out content-phrase set of `evals/fill/finals/` with one jevify binary on one
backend, and score the runs against the gold.

    python3 scripts/eval_fill_finals.py run --binary target/release/jevify --label head \
        --backend typesafe --repos /path/to/clones --out evals/out/fill-finals/head-typesafe.jsonl
    python3 scripts/eval_fill_finals.py score evals/out/fill-finals/*.jsonl

`run` executes every case with `--dry-run --json`, `JEVIFY_NO_CACHE=1` and
`JEVIFY_DECISION=round_one`, inside the clone of the case's repository under `--repos/<repo>`,
which must be checked out at the case's pin (a clone at another commit is `not_run`, never
scored). One row per case: the handle the verb chose or `none`, the score at the gate, the
number of requests, whether the finals ran (a second request after the names round), and the
top of the names round. Nothing a case describes is ever executed: the command is `echo`.

The typesafe backend needs `TYPESAFE_API_KEY_FILE` in the environment; the classifier backend
runs with both key variables removed. `score` reads `evals/fill/finals/gold/gold.jsonl` and
prints, per run file: cases, finals run, coverage, accuracy on the decided, abstentions (right
ones in parentheses), false actions and total requests.
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CASES = REPO / "evals" / "fill" / "finals" / "cases.jsonl"
GOLD = REPO / "evals" / "fill" / "finals" / "gold" / "gold.jsonl"
ABSTAIN_GOLD = {"none", "ambiguous"}


def read_jsonl(path):
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def env_for(backend):
    env = {k: v for k, v in os.environ.items() if not k.startswith("JEVIFY_")}
    env["JEVIFY_NO_CACHE"] = "1"
    env["JEVIFY_DECISION"] = "round_one"
    env["JEVIFY_BACKEND"] = backend
    if backend == "classifier":
        env.pop("TYPESAFE_API_KEY_FILE", None)
        env.pop("TYPESAFE_API_KEY", None)
    elif "TYPESAFE_API_KEY_FILE" not in env and "TYPESAFE_API_KEY" not in env:
        sys.exit("typesafe backend: set TYPESAFE_API_KEY_FILE in the environment")
    return env


def head_of(clone):
    head = (clone / ".git" / "HEAD").read_text().strip()
    if head.startswith("ref: "):
        ref = clone / ".git" / head[5:]
        return ref.read_text().strip() if ref.is_file() else ""
    return head


def run_case(binary, case, env, repos):
    row = {"id": case["id"], "label": None, "backend": env["JEVIFY_BACKEND"], "model": None,
           "decision": "not_run", "score": None, "exit_code": None, "requests": None,
           "finals_ran": None, "names": None, "reason": None}
    clone = repos / case["repo"]
    if not (clone / ".git").exists() or head_of(clone) != case["pin"]:
        row["reason"] = f"no clone of {case['repo']} at {case['pin'][:7]} under {clone}"
        return row
    proc = subprocess.run([str(binary), *case["argv"]], cwd=clone, env=env, capture_output=True,
                          text=True, timeout=600, check=False)
    row["exit_code"] = proc.returncode
    try:
        envelope = json.loads(proc.stdout)
    except json.JSONDecodeError:
        row["reason"] = proc.stderr.strip().splitlines()[-1:] or "no envelope"
        return row
    meta = envelope.get("meta") or {}
    row["model"] = meta.get("model")
    row["requests"] = meta.get("requests")
    marker = ((envelope.get("data") or {}).get("markers") or [{}])[0]
    row["score"] = marker.get("p")
    if proc.returncode == 0 and marker.get("handle") is not None:
        row["decision"] = case["prefix"] + marker["handle"]
    elif proc.returncode == 3:
        row["decision"] = "none"
        row["reason"] = marker.get("reason") or (envelope.get("data") or {}).get("reason")
    else:
        row["reason"] = ((envelope.get("error") or {}).get("kind")
                         or proc.stderr.strip().splitlines()[-1:])
    # A single names window then a finals request is two requests; a decisive names round
    # that skipped the finals is one. `round_one` is opt-in and absent on older binaries.
    rounds = (meta.get("decision") or {}).get("round_one") or []
    if rounds and rounds[0].get("windows"):
        window = rounds[0]["windows"][0]
        ranks = window.get("ranks") or []
        row["names"] = {"best": ranks[0]["p"] if ranks else None,
                        "next": ranks[1]["p"] if len(ranks) > 1 else None,
                        "none": window.get("none")}
        row["finals_ran"] = bool(rounds[0].get("finalists"))
    if row["finals_ran"] is None and row["requests"] is not None:
        row["finals_ran"] = row["requests"] >= 2
    return row


def cmd_run(args):
    binary = Path(args.binary).resolve()
    repos = Path(args.repos).resolve()
    env = env_for(args.backend)
    cases = read_jsonl(CASES)
    if args.only:
        cases = [c for c in cases if c["id"] in args.only]
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "w") as f:
        for case in cases:
            row = run_case(binary, case, env, repos)
            row["label"] = args.label
            f.write(json.dumps(row) + "\n")
            f.flush()
            print(f"{row['id']}: {row['decision']} {row['score']} req={row['requests']} "
                  f"finals={row['finals_ran']} names={row['names']}", file=sys.stderr)


def score_rows(rows, gold):
    scored = [r for r in rows if r["decision"] != "not_run"]
    decided = [r for r in scored if r["decision"] != "none"]
    abstained = [r for r in scored if r["decision"] == "none"]
    right_abstentions = sum(1 for r in abstained if gold[r["id"]] in ABSTAIN_GOLD)
    correct = sum(1 for r in decided if r["decision"] == gold[r["id"]])
    false_actions = len(decided) - correct
    finals = sum(1 for r in scored if r["finals_ran"])
    requests = sum(r["requests"] or 0 for r in scored)
    return {"n": len(scored), "not_run": len(rows) - len(scored), "finals": finals,
            "decided": len(decided), "correct": correct, "abstained": len(abstained),
            "right_abstentions": right_abstentions, "false_actions": false_actions,
            "requests": requests}


def cmd_score(args):
    gold = {g["id"]: g["gold"] for g in read_jsonl(GOLD)}
    print("| Run | Backend | n | finals ran | coverage | accuracy | abstentions (right) "
          "| false actions | requests |")
    print("|:---|:---|---:|---:|---:|---:|---:|---:|---:|")
    for path in args.runs:
        rows = read_jsonl(path)
        if not rows:
            continue
        s = score_rows(rows, gold)
        cov = s["decided"] / s["n"] if s["n"] else 0.0
        acc = s["correct"] / s["decided"] if s["decided"] else 0.0
        print(f"| {rows[0]['label']} | {rows[0]['backend']} | {s['n']} | {s['finals']} "
              f"| {cov:.2f} | {acc:.2f} | {s['abstained']} ({s['right_abstentions']}) "
              f"| {s['false_actions']} | {s['requests']} |")
        if args.verbose:
            for r in rows:
                mark = ("ok" if r["decision"] == gold[r["id"]]
                        else "abstain" if r["decision"] == "none" else "WRONG")
                print(f"    {r['id']}: {mark} {r['decision']} {r['score']} "
                      f"(gold {gold[r['id']]}) req={r['requests']} names={r['names']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run")
    run.add_argument("--binary", required=True)
    run.add_argument("--label", required=True, help="a name for the binary, e.g. head or pre")
    run.add_argument("--backend", required=True, choices=["typesafe", "classifier"])
    run.add_argument("--repos", required=True, help="directory holding one clone per repo")
    run.add_argument("--out", required=True)
    run.add_argument("--only", nargs="*", help="case ids to run")
    run.set_defaults(func=cmd_run)
    score = sub.add_parser("score")
    score.add_argument("runs", nargs="+")
    score.add_argument("-v", "--verbose", action="store_true")
    score.set_defaults(func=cmd_score)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()

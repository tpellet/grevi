# /// script
# requires-python = ">=3.11"
# ///
"""Run the frozen validation set of `evals/validation/` against one backend and write the run
that `scripts/validation_gold.py score` reads.

    python3 scripts/validation_run.py --backend typesafe   --out evals/out/validation/typesafe.jsonl
    python3 scripts/validation_run.py --backend classifier --out evals/out/validation/classifier.jsonl

The run never reads the gold: it executes each case's `argv` with the release binary, reads the
decision out of the `--json` envelope (`data` for the answer, `meta.decision` for the backend, the
requested and answering model, the threshold and the scores at the gate) and writes one line per
case. A case the run cannot execute (a missing clone, a tool absent from the PATH, a backend
error, or an answer from a model other than Jev) is `decision: not_run` with the reason next to
it, never a result.

TypeSafe reads its key from `TYPESAFE_API_KEY_FILE` in the environment; this script never opens
that file. The classifier backend runs with both key variables removed from the environment. The
cache is off for every case (`JEVIFY_NO_CACHE=1`), so every decision is a live answer.

Environment-pinned cases (`env` in the manifest) run inside a clone of the named repository at
the named commit under `--repos` (default `evals/out/validation/repos/<name>`); a clone whose
HEAD is not the pinned commit gives `not_run`. `filter` and `label` cases share one `run`: the
verb executes once per run and each record case reads its own verdict.

Each raw envelope is kept under `<out dir>/raw/<backend>/<case id>.json` for the write-up.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
VALIDATION = REPO / "evals" / "validation"
sys.path.insert(0, str(REPO / "scripts"))
from validation_gold import load_cases

JEV_PREFIX = "jev-"


def env_for(backend):
    env = {k: v for k, v in os.environ.items()
           if k not in ("TYPESAFE_API_KEY", "JEVIFY_BACKEND", "JEVIFY_MODEL", "JEVIFY_THRESHOLD")}
    env["JEVIFY_NO_CACHE"] = "1"
    if backend == "classifier":
        env.pop("TYPESAFE_API_KEY_FILE", None)
        env["JEVIFY_BACKEND"] = "classifier"
    else:
        if "TYPESAFE_API_KEY_FILE" not in env:
            sys.exit("typesafe needs TYPESAFE_API_KEY_FILE in the environment")
        env["JEVIFY_BACKEND"] = "typesafe"
    return env


def run_jevify(binary, argv, env, cwd, stdin_path):
    started = time.monotonic()
    if stdin_path:
        with open(stdin_path, "rb") as stdin:
            proc = subprocess.run([binary, *argv], stdin=stdin, capture_output=True, cwd=cwd,
                                  env=env, timeout=600, check=False)
    else:
        proc = subprocess.run([binary, *argv], stdin=subprocess.DEVNULL, capture_output=True,
                              cwd=cwd, env=env, timeout=600, check=False)
    elapsed = time.monotonic() - started
    try:
        envelope = json.loads(proc.stdout.decode("utf-8", "replace"))
    except (json.JSONDecodeError, ValueError):
        envelope = None
    return proc.returncode, envelope, proc.stderr.decode("utf-8", "replace"), elapsed


def gate_score(verb, gate):
    """The score the verb compares to the threshold: the Noul for yes/no verbs, the best Choice
    probability for the selection verbs (route has no Noul), both kept in `gate`."""
    if gate is None:
        return None
    if verb in ("is", "filter"):
        return gate.get("any")
    return gate.get("best")


def base_row(case, backend):
    return {"id": case["id"], "backend": backend, "model": "unknown", "requested_model": None,
            "threshold": None, "decision": "not_run", "score": None, "exit_code": None,
            "gate": None, "reason": None, "requests": 0, "questions": 0, "elapsed_ms": None}


def fill_meta(row, exit_code, envelope, elapsed):
    row["exit_code"] = exit_code
    row["elapsed_ms"] = int(elapsed * 1000)
    if envelope is None:
        row["reason"] = "no envelope on stdout"
        return None
    meta = envelope.get("meta") or {}
    decision = meta.get("decision") or {}
    model = decision.get("model") or {}
    row["model"] = model.get("answering", "unknown")
    row["requested_model"] = model.get("requested")
    row["threshold"] = decision.get("threshold")
    row["backend"] = decision.get("backend") or row["backend"]
    row["requests"] = meta.get("requests", 0)
    row["questions"] = (meta.get("telemetry") or {}).get("semantic_questions", 0)
    error = envelope.get("error")
    if error:
        row["reason"] = f"{error.get('kind')}: {error.get('message')}"[:300]
        return None
    if not row["model"].startswith(JEV_PREFIX):
        row["reason"] = f"answered by {row['model']}, not Jev"
        return None
    return decision.get("gates") or []


def decide_single(case, row, exit_code, envelope, elapsed):
    verb = case["verb"]
    gates = fill_meta(row, exit_code, envelope, elapsed)
    if gates is None:
        return
    data = envelope.get("data") or {}
    gate = gates[0] if gates else None
    row["gate"] = gate
    row["score"] = gate_score(verb, gate)
    if exit_code not in (0, 1, 3):
        row["reason"] = f"exit {exit_code}"
        return
    if verb == "fill":
        markers = data.get("markers") or []
        if exit_code == 0 and data.get("argv"):
            row["decision"] = markers[0]["handle"] if markers else "none"
        else:
            row["decision"] = "none"
            row["reason"] = data.get("reason") or (markers[0].get("reason") if markers else None)
    elif verb == "pick":
        matches = data.get("matches") or []
        row["decision"] = str(matches[0]["line"]) if exit_code == 0 and matches else "none"
        if exit_code != 0:
            row["reason"] = data.get("reason")
    elif verb == "pick --from":
        matches = data.get("matches") or []
        if exit_code == 0 and matches:
            handle = matches[0]["text"]
            if case["argv"][2] == "commit":
                # the lister hands out the full sha; the answer space is the short one
                handle = handle.split()[0][:7]
            row["decision"] = handle
        else:
            row["decision"] = "none"
            row["reason"] = data.get("reason")
    elif verb == "why":
        causes = data.get("causes") or []
        row["decision"] = str(causes[0]["line"]) if exit_code == 0 and causes else "none"
    elif verb == "is":
        row["decision"] = data.get("verdict") or "unsure"
    elif verb == "route":
        row["decision"] = data.get("tool") or "none"
    else:
        row["reason"] = f"unknown verb {verb}"


def run_grouped(binary, cases, env, backend, raw_dir):
    """`filter` and `label` cases of one run: the verb executes once, every record reads its own
    verdict; the gate list is in input order, one per distinct record."""
    first = cases[0]
    stdin = VALIDATION / first["stdin"]
    argv = [first["argv"][0], "--json", *first["argv"][1:]]
    exit_code, envelope, stderr, elapsed = run_jevify(binary, argv, env, str(REPO), stdin)
    (raw_dir / f"{first['run']}.json").write_text(json.dumps(
        {"argv": argv, "exit_code": exit_code, "stderr": stderr, "envelope": envelope}, indent=1))
    rows = []
    probe = base_row(first, backend)
    gates = fill_meta(probe, exit_code, envelope, elapsed)
    data = (envelope or {}).get("data") or {}
    records = {r["ordinal"]: r for r in (data.get("records") or [])}
    for case in cases:
        row = dict(probe, id=case["id"], decision="not_run")
        if gates is None or exit_code not in (0, 1, 3):
            if row["reason"] is None:
                row["reason"] = f"exit {exit_code}"
            rows.append(row)
            continue
        ordinal = case["record_ordinal"]
        gate = gates[ordinal - 1] if ordinal - 1 < len(gates) else None
        row["gate"] = gate
        row["score"] = gate_score(case["verb"], gate)
        entry = records.get(ordinal)
        if case["verb"] == "filter":
            verdict = entry["verdict"] if entry else "no"
            row["decision"] = {"yes": "keep", "no": "drop", "unsure": "unsure"}[verdict]
            if entry is not None:
                row["score"] = entry.get("p", row["score"])
        else:
            row["decision"] = entry["label"] if entry else "?"
            if entry is not None:
                row["score"] = entry.get("p", row["score"])
        rows.append(row)
    return rows


def run_single(binary, case, env, backend, repos, raw_dir):
    row = base_row(case, backend)
    verb = case["verb"]
    cwd = str(REPO)
    if "env" in case:
        pin = case["env"]
        clone = repos / pin["repo"].split("/")[-1]
        head = None
        if (clone / ".git" / "HEAD").is_file():
            # a detached checkout of the pinned commit: HEAD holds the sha itself
            head = (clone / ".git" / "HEAD").read_text().strip()
        if head != pin["sha"]:
            row["reason"] = f"no clone of {pin['repo']} at {pin['sha'][:7]} under {clone}"
            return row
        cwd = str(clone)
    if verb == "route":
        missing = [t for t in case.get("requires", []) if shutil.which(t) is None]
        if missing:
            row["reason"] = f"tools absent from PATH: {', '.join(missing)}"
            return row
    argv = list(case["argv"])
    argv.insert(1, "--json")
    stdin = VALIDATION / case["stdin"] if "stdin" in case else None
    if "candidates" in case:
        argv[1:1] = ["--candidates", str(VALIDATION / case["candidates"])]
    if "context" in case:
        argv = [str(VALIDATION / case["context"]) if a == "CONTEXT" else a for a in argv]
    if verb == "why":
        argv.insert(1, "--no-save")
    exit_code, envelope, stderr, elapsed = run_jevify(binary, argv, env, cwd, stdin)
    (raw_dir / f"{case['id']}.json").write_text(json.dumps(
        {"argv": argv, "cwd": cwd, "exit_code": exit_code, "stderr": stderr, "envelope": envelope}, indent=1))
    decide_single(case, row, exit_code, envelope, elapsed)
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", required=True, choices=["typesafe", "classifier"])
    parser.add_argument("--out", required=True, help="the run file, one JSON object per case")
    parser.add_argument("--binary", default=str(REPO / "target" / "release" / "jevify"))
    parser.add_argument("--repos", default=str(REPO / "evals" / "out" / "validation" / "repos"))
    parser.add_argument("--split", default="all", choices=["calibration", "validation", "all"])
    parser.add_argument("--verb", action="append", help="only these verbs (repeatable)")
    parser.add_argument("--only", action="append", help="only these case ids (repeatable)")
    parser.add_argument("--stop-after-questions", type=int, default=0,
                        help="stop once this many semantic questions were sent (0: no limit)")
    args = parser.parse_args()

    cases, _ = load_cases()
    if args.split != "all":
        cases = [c for c in cases if c["split"] == args.split]
    if args.verb:
        cases = [c for c in cases if c["verb"] in args.verb]
    if args.only:
        cases = [c for c in cases if c["id"] in args.only]
    env = env_for(args.backend)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    raw_dir = out.parent / "raw" / args.backend
    raw_dir.mkdir(parents=True, exist_ok=True)
    version = subprocess.run([args.binary, "--version"], capture_output=True, text=True,
                             check=False, timeout=60).stdout.strip()
    print(f"{version}, backend {args.backend}, {len(cases)} cases -> {out}", file=sys.stderr)

    grouped = defaultdict(list)
    singles = []
    for case in cases:
        if "run" in case:
            grouped[case["run"]].append(case)
        else:
            singles.append(case)
    questions = 0
    rows = []
    with out.open("w") as sink:
        def emit(new_rows):
            nonlocal questions
            for row in new_rows:
                sink.write(json.dumps(row) + "\n")
                sink.flush()
                rows.append(row)
                print(f"  {row['id']:<44} {row['decision']:<28} score={row['score']} "
                      f"exit={row['exit_code']} model={row['model']}"
                      + (f" ({row['reason']})" if row["reason"] else ""), file=sys.stderr)
            if new_rows:
                questions += new_rows[0]["questions"]
        for group in grouped.values():
            if args.stop_after_questions and questions >= args.stop_after_questions:
                break
            emit(run_grouped(args.binary, group, env, args.backend, raw_dir))
        for case in singles:
            if args.stop_after_questions and questions >= args.stop_after_questions:
                print(f"stopping: {questions} questions sent", file=sys.stderr)
                break
            emit([run_single(args.binary, case, env, args.backend, Path(args.repos), raw_dir)])
    not_run = sum(1 for r in rows if r["decision"] == "not_run")
    print(f"{len(rows)} rows, {not_run} not_run, {questions} semantic questions sent", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())

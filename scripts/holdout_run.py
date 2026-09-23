# /// script
# requires-python = ">=3.11"
# ///
"""Run the source-held-out set of `evals/holdout/` against one backend and write the run that
`scripts/holdout_score.py` reads.

    python3 scripts/holdout_run.py --backend classifier --out evals/out/holdout/classifier.jsonl
    python3 scripts/holdout_run.py --backend typesafe   --out evals/out/holdout/typesafe.jsonl

The run never opens the gold. It executes each case's `argv` with the release binary, reads the
answer out of the `--json` envelope, and writes one line per case with the exit code, the answer,
the wall time and the answering model the envelope reports. A case the run cannot execute (a
missing clone, a backend error, an answer from a model other than Jev) is `decision: not_run`
with the reason beside it, never a result.

The cache is off for every case (`JEVIFY_NO_CACHE=1`), so every decision is a live answer.
TypeSafe reads its key from `TYPESAFE_API_KEY_FILE`; this script never opens that file. The
keyless backend runs with both key variables removed.

`route` cases read the frozen inventory the case names through `JEVIFY_INVENTORY_FILE` instead of
the machine's PATH, so the same 24 tools are on offer on every machine. `fill` cases with an
`env` run inside a clone of the pinned repository under `--repos`; `--clone` fetches the clones
first. `filter` cases share one `run`: the verb executes once and each record reads its verdict.

The summary at the end counts the rows, the `not_run` rows, the HTTP 402 and 502 responses seen,
and the semantic questions sent. It says nothing about whether an answer was right.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HOLDOUT = REPO / "evals" / "holdout"
JEV_PREFIX = "jev-"
ANSWER_FIELDS = ("gold", "answer", "expected", "label", "label_a", "label_b", "adjudicated")


class ManifestError(Exception):
    """A manifest that cannot be run as written; nothing runs before it is fixed."""


def load_cases():
    """The manifest, with every input checked against the digest recorded beside it."""
    import hashlib
    import hmac

    sources = {}
    for ln, raw in enumerate((HOLDOUT / "sources.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        source = json.loads(raw)
        for field in ("id", "kind", "license", "collected"):
            if not source.get(field):
                raise ManifestError(f"sources.jsonl:{ln}: source lacks {field!r}")
        sources[source["id"]] = source
    cases, seen = [], set()
    for ln, raw in enumerate((HOLDOUT / "cases.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        case = json.loads(raw)
        cid = case.get("id")
        if not cid or cid in seen:
            raise ManifestError(f"cases.jsonl:{ln}: missing or duplicate id {cid!r}")
        seen.add(cid)
        for field in ANSWER_FIELDS:
            if field in case:
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} carries its own answer "
                                    f"({field})")
        for field in ("stdin", "candidates", "inventory"):
            if field not in case:
                continue
            path = HOLDOUT / case[field]
            if not path.is_file():
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} names a missing input {path}")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if not hmac.compare_digest(digest, str(case.get(field + "_sha256"))):
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} input {case[field]} hashes "
                                    f"{digest}, not what the manifest records")
        cases.append(case)
    if not cases:
        raise ManifestError("cases.jsonl: no cases")
    return cases, sources


def env_for(backend):
    env = {k: v for k, v in os.environ.items()
           if k not in ("TYPESAFE_API_KEY", "JEVIFY_BACKEND", "JEVIFY_MODEL", "JEVIFY_THRESHOLD",
                        "JEVIFY_INVENTORY_FILE")}
    env["JEVIFY_NO_CACHE"] = "1"
    if backend == "classifier":
        env.pop("TYPESAFE_API_KEY_FILE", None)
        env["JEVIFY_BACKEND"] = "classifier"
    else:
        if "TYPESAFE_API_KEY_FILE" not in env:
            sys.exit("typesafe needs TYPESAFE_API_KEY_FILE in the environment")
        env["JEVIFY_BACKEND"] = "typesafe"
    return env


def clone(cases, repos):
    """Check out every pinned repository the manifest names, detached at its commit."""
    repos.mkdir(parents=True, exist_ok=True)
    pins = {(c["env"]["repo"], c["env"]["sha"]) for c in cases if "env" in c}
    for repo, sha in sorted(pins):
        target = repos / repo.split("/")[-1]
        if not (target / ".git").is_dir():
            subprocess.run(["git", "clone", "--quiet", f"https://github.com/{repo}.git",
                            str(target)], check=True, timeout=1800)
        subprocess.run(["git", "-C", str(target), "fetch", "--quiet", "origin", sha],
                       check=False, timeout=1800)
        subprocess.run(["git", "-C", str(target), "checkout", "--quiet", "--detach", sha],
                       check=True, timeout=600)
        print(f"{repo} at {sha[:7]} -> {target}", file=sys.stderr)


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
    """The score the verb compares to the threshold: the Noul for the yes/no verbs, the best
    Choice probability for the selection verbs."""
    if gate is None:
        return None
    return gate.get("any") if verb == "filter" else gate.get("best")


def base_row(case, backend):
    return {"id": case["id"], "verb": case["verb"], "backend": backend, "model": "unknown",
            "requested_model": None, "threshold": None, "decision": "not_run", "score": None,
            "exit_code": None, "reason": None, "requests": 0, "questions": 0, "elapsed_ms": None,
            "http_402": 0, "http_502": 0}


def count_http(row, envelope, stderr):
    """402 and 502 are reported in the error message and on stderr, not as a field; count both
    wherever the run can see them."""
    error = (envelope or {}).get("error") or {}
    text = f"{error.get('message', '')} {error.get('hint', '')} {stderr}"
    row["http_402"] = len(re.findall(r"HTTP 402", text))
    row["http_502"] = len(re.findall(r"HTTP 502", text))


def fill_meta(row, exit_code, envelope, stderr, elapsed):
    row["exit_code"] = exit_code
    row["elapsed_ms"] = int(elapsed * 1000)
    count_http(row, envelope, stderr)
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


def decide_single(case, row, exit_code, envelope, stderr, elapsed):
    verb = case["verb"]
    gates = fill_meta(row, exit_code, envelope, stderr, elapsed)
    if gates is None:
        return
    data = envelope.get("data") or {}
    gate = gates[0] if gates else None
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
    elif verb == "why":
        causes = data.get("causes") or []
        row["decision"] = str(causes[0]["line"]) if exit_code == 0 and causes else "none"
    elif verb == "route":
        row["decision"] = data.get("tool") or "none"
    else:
        row["reason"] = f"unknown verb {verb}"


def run_grouped(binary, cases, env, backend, raw_dir):
    """The `filter` cases of one run: the verb executes once and every record reads its own
    verdict; the gate list is in input order, one per distinct record."""
    first = cases[0]
    argv = [first["argv"][0], "--json", *first["argv"][1:]]
    exit_code, envelope, stderr, elapsed = run_jevify(
        binary, argv, env, str(REPO), HOLDOUT / first["stdin"])
    (raw_dir / f"{first['run']}.json").write_text(json.dumps(
        {"argv": argv, "exit_code": exit_code, "stderr": stderr, "envelope": envelope}, indent=1))
    probe = base_row(first, backend)
    gates = fill_meta(probe, exit_code, envelope, stderr, elapsed)
    data = (envelope or {}).get("data") or {}
    records = {r["ordinal"]: r for r in (data.get("records") or [])}
    rows = []
    for case in cases:
        row = dict(probe, id=case["id"], verb=case["verb"], decision="not_run")
        if gates is None or exit_code not in (0, 1, 3):
            row["reason"] = row["reason"] or f"exit {exit_code}"
            rows.append(row)
            continue
        ordinal = case["record_ordinal"]
        gate = gates[ordinal - 1] if ordinal - 1 < len(gates) else None
        row["score"] = gate_score(case["verb"], gate)
        entry = records.get(ordinal)
        verdict = entry["verdict"] if entry else "no"
        row["decision"] = {"yes": "keep", "no": "drop", "unsure": "unsure"}[verdict]
        if entry is not None:
            row["score"] = entry.get("p", row["score"])
        rows.append(row)
    return rows


def run_single(binary, case, env, backend, repos, raw_dir):
    row = base_row(case, backend)
    cwd = str(REPO)
    env = dict(env)
    if "env" in case:
        pin = case["env"]
        checkout = repos / pin["repo"].split("/")[-1]
        head = None
        if (checkout / ".git" / "HEAD").is_file():
            # a detached checkout of the pinned commit: HEAD holds the sha itself
            head = (checkout / ".git" / "HEAD").read_text().strip()
        if head != pin["sha"]:
            row["reason"] = (f"no checkout of {pin['repo']} at {pin['sha'][:7]} under "
                             f"{checkout} (--clone makes one)")
            return row
        cwd = str(checkout)
    if "inventory" in case:
        env["JEVIFY_INVENTORY_FILE"] = str(HOLDOUT / case["inventory"])
    argv = list(case["argv"])
    argv.insert(1, "--json")
    if case["verb"] == "why":
        argv.insert(1, "--no-save")
    if "candidates" in case:
        argv[1:1] = ["--candidates", str(HOLDOUT / case["candidates"])]
    stdin = HOLDOUT / case["stdin"] if "stdin" in case else None
    exit_code, envelope, stderr, elapsed = run_jevify(binary, argv, env, cwd, stdin)
    (raw_dir / f"{case['id']}.json").write_text(json.dumps(
        {"argv": argv, "cwd": cwd, "exit_code": exit_code, "stderr": stderr,
         "envelope": envelope}, indent=1))
    decide_single(case, row, exit_code, envelope, stderr, elapsed)
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--backend", required=True, choices=["typesafe", "classifier"])
    parser.add_argument("--out", required=True, help="the run file, one JSON object per case")
    parser.add_argument("--binary", default=str(REPO / "target" / "release" / "jevify"))
    parser.add_argument("--repos", default=str(REPO / "evals" / "out" / "holdout" / "repos"))
    parser.add_argument("--verb", action="append", help="only these verbs (repeatable)")
    parser.add_argument("--only", action="append", help="only these case ids (repeatable)")
    parser.add_argument("--limit", type=int, default=0, help="run at most this many cases")
    parser.add_argument("--clone", action="store_true",
                        help="check out every pinned repository first, then run")
    args = parser.parse_args()

    cases, _ = load_cases()
    if args.clone:
        clone(cases, Path(args.repos))
    if args.verb:
        cases = [c for c in cases if c["verb"] in args.verb]
    if args.only:
        cases = [c for c in cases if c["id"] in args.only]
    if args.limit:
        cases = cases[:args.limit]
    env = env_for(args.backend)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    raw_dir = out.parent / "raw" / args.backend
    raw_dir.mkdir(parents=True, exist_ok=True)
    version = subprocess.run([args.binary, "--version"], capture_output=True, text=True,
                             check=False, timeout=60).stdout.strip()
    print(f"{version}, backend {args.backend}, {len(cases)} cases -> {out}", file=sys.stderr)

    grouped, singles = defaultdict(list), []
    for case in cases:
        (grouped[case["run"]] if "run" in case else singles).append(case)
    rows = []
    with out.open("w") as sink:
        def emit(new_rows):
            for row in new_rows:
                sink.write(json.dumps(row) + "\n")
                sink.flush()
                rows.append(row)
                print(f"  {row['id']:<30} {row['decision']:<34} score={row['score']} "
                      f"exit={row['exit_code']} {row['elapsed_ms']}ms model={row['model']}"
                      + (f" ({row['reason']})" if row["reason"] else ""), file=sys.stderr)
        for group in grouped.values():
            emit(run_grouped(args.binary, group, env, args.backend, raw_dir))
        for case in singles:
            emit([run_single(args.binary, case, env, args.backend, Path(args.repos), raw_dir)])
    not_run = sum(1 for r in rows if r["decision"] == "not_run")
    questions = sum(r["questions"] for r in rows)
    print(f"{len(rows)} rows, {not_run} not_run, {sum(r['http_402'] for r in rows)} HTTP 402, "
          f"{sum(r['http_502'] for r in rows)} HTTP 502, {questions} semantic questions sent",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ManifestError as err:
        print(f"ManifestError: {err}", file=sys.stderr)
        sys.exit(2)

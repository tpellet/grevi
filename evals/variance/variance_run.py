# /// script
# requires-python = ">=3.11"
# ///
"""Measure how much a jevify answer moves when nothing about the question moves.

Two measurements over a fixed subset of `evals/holdout/`, each written as one JSON object per
answer and scored by `variance_score.py`:

    python3 evals/variance/variance_run.py repeat --backend classifier \
        --out evals/out/variance/repeat-classifier.jsonl
    python3 evals/variance/variance_run.py order  --backend classifier \
        --out evals/out/variance/order-classifier.jsonl

`repeat` runs the same case, byte-identical, five times with the cache off. `order` runs the
same case under eight orders of its candidates: the order the input already has, then seven
shuffles seeded by the case id, written to a scratch copy of the input.

Every answer is recorded by its identity, not by its position: a `pick` line number becomes the
record's text, a `filter` verdict is keyed by the record's text, `fill` and `route` answer with a
handle that is already order-free. Two answers from two orders are therefore comparable.

The cache is off for every call (`JEVIFY_NO_CACHE=1`), so every answer is a live one. TypeSafe
reads its key from `TYPESAFE_API_KEY_FILE`; this script never opens that file. The keyless
backend runs with both key variables removed. The run never opens the gold; `variance_score.py`
does.
"""
import argparse
import json
import os
import random
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
HOLDOUT = REPO / "evals" / "holdout"
JEV_PREFIX = "jev-"

# The fixed subset. Every id is a case of `evals/holdout/cases.jsonl`; the subset is named here
# so that a rerun measures the same questions, and it spans every verb the set carries.
REPEAT_SUBSET = [
    "filter-zox", "filter-jst",                                         # two runs, 20 records
    "pick-jst-01", "pick-jst-03", "pick-jst-05",
    "pick-del-02", "pick-del-05", "pick-zox-03", "pick-zox-06",
    "why-rust-borrow-conflict", "why-py-keyerror", "why-rust-clean-build",
    "why-c-link-undefined", "why-py-recursion", "why-rust-test-assert",
    "route-ctr-02", "route-med-03", "route-dat-01",
    "route-med-06", "route-ctr-07", "route-dat-06",
    "fill-jst-01", "fill-jst-02", "fill-jst-03", "fill-jst-04",
]
# Only a case whose candidates arrive as a list has an order to shuffle: the records on stdin,
# the frozen candidate file, the tool inventory. A `why` log is not a candidate list.
ORDER_SUBSET = [
    "filter-zox", "filter-jst",
    "pick-jst-03", "pick-del-05", "pick-zox-03", "pick-zox-06",
    "fill-jst-01", "fill-jst-03",
    "route-ctr-02", "route-med-03", "route-dat-01",
]
TRIALS = 5
ORDERS = 8


class ManifestError(Exception):
    """A subset that cannot be run as written; nothing runs before it is fixed."""


def load_cases():
    """The holdout manifest, grouped the way a run executes it: a `filter` run is one unit whose
    records each read their own verdict, every other case is a unit of its own."""
    import hashlib
    import hmac

    units, by_id = {}, {}
    for ln, raw in enumerate((HOLDOUT / "cases.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        case = json.loads(raw)
        for field in ("gold", "answer", "expected"):
            if field in case:
                raise ManifestError(f"cases.jsonl:{ln}: case {case['id']} carries its own answer")
        for field in ("stdin", "candidates", "inventory"):
            if field not in case:
                continue
            path = HOLDOUT / case[field]
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if not hmac.compare_digest(digest, str(case.get(field + "_sha256"))):
                raise ManifestError(f"cases.jsonl:{ln}: {case['id']} input {case[field]} hashes "
                                    f"{digest}, not what the manifest records")
        by_id[case["id"]] = case
        units.setdefault(case.get("run", case["id"]), []).append(case)
    return units, by_id


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


def permute(unit, order, scratch):
    """The case's candidate list under one order, written where the verb will read it.

    Order 0 is the list as the input file has it and reads the frozen file itself. Orders 1..7
    are shuffles seeded by the unit's name and the order number, so a rerun produces the same
    eight orders.
    """
    case = unit[0]
    field = ("stdin" if "stdin" in case else
             "candidates" if "candidates" in case else "inventory")
    source = HOLDOUT / case[field]
    if order == 0:
        return field, source
    rng = random.Random(f"{case.get('run', case['id'])}:{order}")
    target = scratch / f"{case.get('run', case['id'])}.{order}{source.suffix}"
    if field == "inventory":
        items = json.loads(source.read_text())
        rng.shuffle(items)
        target.write_text(json.dumps(items, indent=1) + "\n")
    else:
        lines = source.read_text().splitlines()
        rng.shuffle(lines)
        target.write_text("\n".join(lines) + "\n")
    return field, target


def run_jevify(binary, argv, env, stdin_path):
    started = time.monotonic()
    if stdin_path:
        with open(stdin_path, "rb") as stdin:
            proc = subprocess.run([binary, *argv], stdin=stdin, capture_output=True,
                                  cwd=str(REPO), env=env, timeout=600, check=False)
    else:
        proc = subprocess.run([binary, *argv], stdin=subprocess.DEVNULL, capture_output=True,
                              cwd=str(REPO), env=env, timeout=600, check=False)
    elapsed = time.monotonic() - started
    try:
        envelope = json.loads(proc.stdout.decode("utf-8", "replace"))
    except (json.JSONDecodeError, ValueError):
        envelope = None
    return proc.returncode, envelope, proc.stderr.decode("utf-8", "replace"), elapsed


def answers(unit, records, exit_code, envelope):
    """Every answer the call produced, each named by an identity that survives a reshuffle.

    A `pick` match and a `why` cause come back as a line number; `pick` reads on the permuted
    list, so its line becomes the record's own text, while a `why` log is never permuted and its
    line is the identity. `fill` and `route` answer with a handle already.
    """
    case = unit[0]
    verb, data = case["verb"], (envelope or {}).get("data") or {}
    gates = (((envelope or {}).get("meta") or {}).get("decision") or {}).get("gates") or []
    gate = gates[0] if gates else {}
    if verb == "filter":
        # a dropped record is not listed in the envelope: what the run reports is what it keeps
        # and what it is unsure about, so an absent ordinal is a `drop`
        seen = {r["ordinal"]: r for r in (data.get("records") or [])}
        out = []
        for member in unit:
            ordinal = records.index(member["record"]) + 1 if member["record"] in records else -1
            entry = seen.get(ordinal) or {}
            out.append({"key": member["id"], "identity": member["record"],
                        "answer": "not_run" if ordinal < 0 else
                                  {"yes": "keep", "unsure": "unsure"}
                                  .get(entry.get("verdict"), "drop"),
                        "score": entry.get("p")})
        return out
    if verb == "pick":
        matches = data.get("matches") or []
        answer = (records[matches[0]["line"] - 1] if exit_code == 0 and matches else "none")
    elif verb == "why":
        causes = data.get("causes") or []
        answer = str(causes[0]["line"]) if exit_code == 0 and causes else "none"
    elif verb == "fill":
        markers = data.get("markers") or []
        answer = (markers[0]["handle"] if exit_code == 0 and data.get("argv") and markers
                  else "none")
    elif verb == "route":
        answer = data.get("tool") or "none"
    else:
        answer = "not_run"
    return [{"key": case["id"], "identity": case["id"], "answer": answer,
             "score": gate.get("any") if verb == "filter" else gate.get("best")}]


def execute(binary, unit, env, backend, arm, index, scratch, raw_dir):
    """One call: the unit under one trial or one order, as a list of answer rows."""
    case = unit[0]
    name = case.get("run", case["id"])
    env = dict(env)
    argv = list(case["argv"])
    argv.insert(1, "--json")
    if case["verb"] == "why":
        argv.insert(1, "--no-save")
    stdin = None
    records = []
    if arm == "order":
        field, path = permute(unit, index, scratch)
    else:
        field = ("stdin" if "stdin" in case else
                 "candidates" if "candidates" in case else
                 "inventory" if "inventory" in case else None)
        path = HOLDOUT / case[field] if field else None
    if field == "stdin":
        stdin = path
        records = path.read_text().splitlines()
    elif field == "candidates":
        argv[1:1] = ["--candidates", str(path)]
    elif field == "inventory":
        env["JEVIFY_INVENTORY_FILE"] = str(path)
    exit_code, envelope, stderr, elapsed = run_jevify(binary, argv, env, stdin)
    (raw_dir / f"{name}.{arm}{index}.json").write_text(json.dumps(
        {"argv": argv, "exit_code": exit_code, "stderr": stderr[-4000:],
         "envelope": envelope}, indent=1))
    meta = (envelope or {}).get("meta") or {}
    model = (((meta.get("decision") or {}).get("model")) or {}).get("answering", "unknown")
    error = (envelope or {}).get("error")
    reason = None
    if envelope is None:
        reason = "no envelope on stdout"
    elif error:
        reason = f"{error.get('kind')}: {error.get('message')}"[:200]
    elif not model.startswith(JEV_PREFIX):
        reason = f"answered by {model}, not Jev"
    rows = []
    for answer in answers(unit, records, exit_code, envelope):
        # one call answers every record of a `filter` run, so its questions are counted once,
        # on the first record, and not once per record
        rows.append({"arm": arm, "unit": name, "index": index, "backend": backend,
                     "id": answer["key"], "verb": case["verb"], "identity": answer["identity"],
                     "answer": "not_run" if reason else answer["answer"],
                     "score": answer["score"], "exit_code": exit_code, "model": model,
                     "reason": reason, "elapsed_ms": int(elapsed * 1000),
                     "questions": 0 if rows else
                                  (meta.get("telemetry") or {}).get("semantic_questions", 0),
                     "http_402": 0 if rows else
                                 stderr.count("HTTP 402") + str(error).count("HTTP 402")})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("arm", choices=["repeat", "order"])
    parser.add_argument("--backend", required=True, choices=["typesafe", "classifier"])
    parser.add_argument("--out", required=True)
    parser.add_argument("--binary", default=str(REPO / "target" / "release" / "jevify"))
    parser.add_argument("--scratch", default=None,
                        help="where the permuted copies of the inputs are written")
    parser.add_argument("--unit", action="append", help="only these units (repeatable)")
    args = parser.parse_args()

    units, _ = load_cases()
    wanted = args.unit or (REPEAT_SUBSET if args.arm == "repeat" else ORDER_SUBSET)
    missing = [name for name in wanted if name not in units]
    if missing:
        raise ManifestError(f"the subset names units the manifest does not have: {missing}")
    env = env_for(args.backend)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    raw_dir = out.parent / "raw" / f"{args.arm}-{args.backend}"
    raw_dir.mkdir(parents=True, exist_ok=True)
    scratch = Path(args.scratch or (out.parent / "permuted"))
    scratch.mkdir(parents=True, exist_ok=True)
    version = subprocess.run([args.binary, "--version"], capture_output=True, text=True,
                             check=False, timeout=60).stdout.strip()
    rounds = TRIALS if args.arm == "repeat" else ORDERS
    print(f"{version}, backend {args.backend}, {args.arm}, {len(wanted)} units x {rounds} "
          f"-> {out}", file=sys.stderr)

    rows = []
    with out.open("w") as sink:
        for index in range(rounds):
            for name in wanted:
                for row in execute(args.binary, units[name], env, args.backend, args.arm,
                                   index, scratch, raw_dir):
                    sink.write(json.dumps(row) + "\n")
                    rows.append(row)
                sink.flush()
            done = [r for r in rows if r["index"] == index]
            print(f"  {args.arm} {index}: {len(done)} answers, "
                  f"{sum(1 for r in done if r['answer'] == 'not_run')} not_run, "
                  f"{sum(r['questions'] for r in done)} questions", file=sys.stderr)
    print(f"{len(rows)} answers, {sum(r['questions'] for r in rows)} semantic questions, "
          f"{sum(r['http_402'] for r in rows)} HTTP 402", file=sys.stderr)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ManifestError as err:
        print(f"ManifestError: {err}", file=sys.stderr)
        sys.exit(2)

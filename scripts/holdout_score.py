# /// script
# requires-python = ">=3.11"
# ///
"""Check the source-held-out manifest and score a run of it against the gold.

    python3 scripts/holdout_score.py check
    python3 scripts/holdout_score.py score evals/out/holdout/classifier.jsonl

`check` validates what a run reads before any request is paid for: every input exists and hashes
as the manifest records, every source is declared, no case carries its own answer, and the gold
covers every case. `score` reads `evals/holdout/gold/gold.jsonl` next to a run's decisions and
reports, per backend and verb and separately:

- **coverage**: the decided cases over the scored ones, so an abstention is not a miss by
  default;
- **accuracy on the decided cases**: the correct decisions over the decided ones;
- **abstention correctness**: how many abstentions fell on a case whose gold is `none`,
  `ambiguous` or `unsure`, out of all abstentions;
- **false actions**: a decision that differs from a concrete gold, plus any decision on a case
  whose gold says not to act.

A `why` gold is the inclusive line range of the root-cause block, so a decision inside the block
is correct. A gold of the form `{"any_of": [...]}` accepts any of the named answers: it is what
adjudication records where two answers are equally right. A case the run could not execute is
`not_run` and is counted apart, never as a failure.
"""
import argparse
import hashlib
import hmac
import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "evals" / "holdout"
ABSTENTIONS = {"none", "unsure", "?"}
OPEN_GOLD = {"none", "ambiguous", "unsure", "?"}
ANSWER_FIELDS = ("gold", "answer", "expected", "label", "label_a", "label_b", "adjudicated")


class ManifestError(Exception):
    """A manifest or a gold file that cannot be scored as written."""


def load_cases():
    sources = {}
    for ln, raw in enumerate((ROOT / "sources.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        source = json.loads(raw)
        for field in ("id", "kind", "license", "collected"):
            if not source.get(field):
                raise ManifestError(f"sources.jsonl:{ln}: source lacks {field!r}")
        sources[source["id"]] = source
    cases, seen = [], set()
    for ln, raw in enumerate((ROOT / "cases.jsonl").read_text().splitlines(), 1):
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
            path = ROOT / case[field]
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


def load_gold():
    gold = {}
    for ln, raw in enumerate((ROOT / "gold" / "gold.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        entry = json.loads(raw)
        if "gold" not in entry or "pass_a" not in entry or "pass_b" not in entry:
            raise ManifestError(f"gold.jsonl:{ln}: entry lacks gold, pass_a or pass_b")
        if not entry.get("agreed") and not entry.get("adjudication"):
            raise ManifestError(f"gold.jsonl:{ln}: {entry['id']} disagrees and records no "
                                f"adjudication")
        gold[entry["id"]] = entry
    return gold


def check():
    cases, sources = load_cases()
    gold = load_gold()
    uncovered = [c["id"] for c in cases if c["id"] not in gold]
    if uncovered:
        raise ManifestError(f"{len(uncovered)} cases have no gold: {', '.join(uncovered[:5])}")
    per = defaultdict(lambda: [0, 0])
    for case in cases:
        entry = gold[case["id"]]
        per[case["verb"]][0] += 1
        if isinstance(entry["gold"], str) and entry["gold"] in OPEN_GOLD:
            per[case["verb"]][1] += 1
    agreed = sum(1 for e in gold.values() if e.get("agreed"))
    print(f"{len(cases)} cases, {len(sources)} sources, inputs verified, gold covers every case")
    print(f"the two passes agreed on {agreed} of {len(gold)}; "
          f"{len(gold) - agreed} adjudicated")
    print(f"{'verb':<10}{'cases':>7}{'abstention gold':>18}")
    for verb in sorted(per):
        print(f"{verb:<10}{per[verb][0]:>7}{per[verb][1]:>18}")
    return 0


def hits(decision, gold):
    if isinstance(gold, list):
        try:
            return gold[0] <= int(decision) <= gold[1]
        except (TypeError, ValueError):
            return False
    if isinstance(gold, dict):
        return str(decision) in {str(g) for g in gold["any_of"]}
    return str(decision) == str(gold)


def outcome(decision, gold):
    """decided/correct, decided/wrong (a false action), abstained/right, abstained/missed."""
    if decision == "not_run":
        return "not_run"
    open_gold = isinstance(gold, str) and gold in OPEN_GOLD
    if decision in ABSTENTIONS:
        return "abstained_right" if open_gold else "abstained_missed"
    if open_gold:
        return "false_action"
    return "correct" if hits(decision, gold) else "false_action"


def score(run_path):
    cases, _ = load_cases()
    by_id = {c["id"]: c for c in cases}
    gold = load_gold()
    tally = defaultdict(lambda: defaultdict(int))
    unknown = []
    for raw in Path(run_path).read_text().splitlines():
        if not raw.strip():
            continue
        row = json.loads(raw)
        case = by_id.get(row["id"])
        if case is None:
            unknown.append(row["id"])
            continue
        answer = gold.get(row["id"])
        if answer is None:
            raise ManifestError(f"no gold for case {row['id']}")
        key = (row.get("backend", "unknown"), case["verb"])
        tally[key][outcome(row.get("decision"), answer["gold"])] += 1
        tally[key]["cases"] += 1
        tally[key]["http_402"] += row.get("http_402", 0)
        tally[key]["http_502"] += row.get("http_502", 0)
    print(f"{'backend':<14}{'verb':<9}{'n':>4}{'cov':>7}{'acc':>7}{'abst ok':>9}{'false':>7}"
          f"{'n/r':>5}{'402':>5}{'502':>5}")
    for key in sorted(tally):
        t = tally[key]
        decided = t["correct"] + t["false_action"]
        scored = t["cases"] - t["not_run"]
        abstained = t["abstained_right"] + t["abstained_missed"]
        cov = decided / scored if scored else 0.0
        acc = t["correct"] / decided if decided else 0.0
        abst = f"{t['abstained_right']}/{abstained}" if abstained else "-"
        print(f"{key[0]:<14}{key[1]:<9}{t['cases']:>4}{cov:>7.2f}{acc:>7.2f}{abst:>9}"
              f"{t['false_action']:>7}{t['not_run']:>5}{t['http_402']:>5}{t['http_502']:>5}")
    print("cov: decided over scored · acc: correct over decided · abst ok: right abstentions "
          "over abstentions · false: a decision that was wrong or that the gold says not to make")
    if unknown:
        print(f"not in the set: {', '.join(sorted(unknown))}", file=sys.stderr)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    scorer = sub.add_parser("score")
    scorer.add_argument("run")
    args = parser.parse_args()
    try:
        return check() if args.cmd == "check" else score(args.run)
    except ManifestError as err:
        print(f"ManifestError: {err}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

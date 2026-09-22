# /// script
# requires-python = ">=3.11"
# ///
"""Load, check and score the frozen validation set of `evals/validation/`.

`check` validates the manifest a run reads (`cases.jsonl`): every input exists and hashes as
recorded, every family is declared, no family sits in both splits, and no case carries its answer.
`score` reads the hidden gold of `evals/validation/gold/gold.jsonl` next to a run's decisions and
reports coverage, abstentions and false actions per backend and verb.

    python3 scripts/validation_gold.py check
    python3 scripts/validation_gold.py score run.jsonl [--split validation]

A run writes one JSON object per case, in any order:

    {"id": "pick-val-01", "backend": "typesafe", "model": "jev-1.13.0", "threshold": 0.5,
     "decision": "6", "score": 0.91, "exit_code": 0}

`decision` speaks the answer space of the case's verb: the handle for `fill`, `pick` and
`pick --from` (a path, a short sha, or the 1-based line number of the chosen record), the 1-based
line number for `why`, `keep`/`drop`/`unsure` for `filter`, a label or `?` for `label`, `yes`/`no`/
`unsure` for `is`, a tool name for `route`. An abstention is `none` (`fill`, `pick`, `why`,
`route`), `unsure` (`filter`, `is`) or `?` (`label`). A case a run could not execute is reported
with `"decision": "not_run"` and is counted apart, never as a failure.
"""
import argparse
import hashlib
import hmac
import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "evals" / "validation"
ABSTENTIONS = {"none", "unsure", "?"}
OPEN_GOLD = {"none", "ambiguous", "unsure", "?"}
ANSWER_FIELDS = ("gold", "answer", "expected", "label_a", "label_b", "adjudicated")


class ManifestError(Exception):
    """A manifest that cannot be run as written; nothing is scored before it is fixed."""


def load_cases():
    families = {}
    for ln, raw in enumerate((ROOT / "families.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        try:
            family = json.loads(raw)
        except json.JSONDecodeError as err:
            raise ManifestError(f"families.jsonl:{ln}: not JSON ({err})") from err
        for field in ("id", "split", "source", "license"):
            if not family.get(field):
                raise ManifestError(f"families.jsonl:{ln}: family lacks {field!r}")
        if family["id"] in families:
            raise ManifestError(f"families.jsonl:{ln}: duplicate family {family['id']!r}")
        families[family["id"]] = family
    cases, seen = [], set()
    for ln, raw in enumerate((ROOT / "cases.jsonl").read_text().splitlines(), 1):
        if not raw.strip():
            continue
        try:
            case = json.loads(raw)
        except json.JSONDecodeError as err:
            raise ManifestError(f"cases.jsonl:{ln}: not JSON ({err})") from err
        cid = case.get("id")
        if not cid or cid in seen:
            raise ManifestError(f"cases.jsonl:{ln}: missing or duplicate id {cid!r}")
        seen.add(cid)
        for field in ANSWER_FIELDS:
            if field in case:
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} carries its own answer ({field})")
        family = families.get(case.get("family"))
        if family is None:
            raise ManifestError(f"cases.jsonl:{ln}: case {cid} names an undeclared family")
        if family["split"] != case.get("split"):
            raise ManifestError(f"cases.jsonl:{ln}: case {cid} is in the {case.get('split')} split "
                                f"but its family is {family['split']}")
        for field in ("stdin", "context", "candidates"):
            if field not in case:
                continue
            path = (ROOT / case[field]).resolve()
            if not path.is_file():
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} names a missing input {path}")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if not hmac.compare_digest(digest, str(case.get(field + "_sha256"))):
                raise ManifestError(f"cases.jsonl:{ln}: case {cid} input {case[field]} hashes "
                                    f"{digest}, not what the manifest records")
        cases.append(case)
    if not cases:
        raise ManifestError("cases.jsonl: no cases")
    return cases, families


def load_gold():
    path = ROOT / "gold" / "gold.jsonl"
    gold = {}
    for ln, raw in enumerate(path.read_text().splitlines(), 1):
        if not raw.strip():
            continue
        try:
            entry = json.loads(raw)
        except json.JSONDecodeError as err:
            raise ManifestError(f"{path.name}:{ln}: not JSON ({err})") from err
        gold[entry["id"]] = entry
    return gold


def check():
    cases, families = load_cases()
    per = defaultdict(int)
    for case in cases:
        per[(case["verb"], case["split"])] += 1
    verbs = sorted({case["verb"] for case in cases})
    for verb in verbs:
        for split in ("calibration", "validation"):
            fams = {c["family"] for c in cases if c["verb"] == verb and c["split"] == split}
            if len(fams) < 2:
                print(f"warning: {verb} has {len(fams)} {split} families", file=sys.stderr)
    gold = load_gold()
    uncovered = [c["id"] for c in cases if c["id"] not in gold]
    if uncovered:
        raise ManifestError(f"{len(uncovered)} cases have no gold: {', '.join(uncovered[:5])}")
    print(f"{len(cases)} cases, {len(families)} families, inputs verified, gold covers every case")
    for key in sorted(per):
        print(f"  {key[0]:<12} {key[1]:<12} {per[key]}")
    return 0


def hits(decision, gold):
    """A `why` gold is the inclusive line range of the root-cause block, a `route` gold may accept
    several commands (`{"any_of": [...]}`); anything else is one value."""
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


def score(run_path, split):
    cases, _ = load_cases()
    by_id = {c["id"]: c for c in cases if split in (None, c["split"])}
    gold = load_gold()
    tally = defaultdict(lambda: defaultdict(int))
    unknown = []
    for raw in Path(run_path).read_text().splitlines():
        if not raw.strip():
            continue
        try:
            row = json.loads(raw)
        except json.JSONDecodeError as err:
            raise ManifestError(f"{run_path}: not JSON ({err})") from err
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
    print(f"{'backend':<16}{'verb':<12}{'n':>4}{'cov':>7}{'acc':>7}{'abst':>7}{'false':>7}{'n/r':>5}")
    for key in sorted(tally):
        t = tally[key]
        decided = t["correct"] + t["false_action"]
        scored = t["cases"] - t["not_run"]
        cov = decided / scored if scored else 0.0
        acc = t["correct"] / decided if decided else 0.0
        abst = (t["abstained_right"] + t["abstained_missed"]) / scored if scored else 0.0
        print(f"{key[0]:<16}{key[1]:<12}{t['cases']:>4}{cov:>7.2f}{acc:>7.2f}{abst:>7.2f}"
              f"{t['false_action']:>7}{t['not_run']:>5}")
    if unknown:
        print(f"not in the {split or 'whole'} set: {', '.join(sorted(unknown))}", file=sys.stderr)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    scorer = sub.add_parser("score")
    scorer.add_argument("run")
    scorer.add_argument("--split", default="validation", choices=["calibration", "validation", "all"])
    args = parser.parse_args()
    try:
        if args.cmd == "check":
            return check()
        return score(args.run, None if args.split == "all" else args.split)
    except ManifestError as err:
        print(f"ManifestError: {err}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

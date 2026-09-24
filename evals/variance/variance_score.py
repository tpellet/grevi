# /// script
# requires-python = ">=3.11"
# ///
"""Score the runs `variance_run.py` writes, per verb and per backend.

    python3 evals/variance/variance_score.py repeat evals/out/variance/repeat-*.jsonl
    python3 evals/variance/variance_score.py order  evals/out/variance/order-*.jsonl

`repeat` reports, per verb and backend, how many questions were asked five times, on how many of
them the five answers were not all the same (the flip rate), and for `filter` the union and the
intersection of the kept set across the five runs.

`order` reports, per verb and backend, how many (question, order) pairs were run, on how many
questions the eight orders did not all agree, and the direction the disagreement takes: an order
whose answer is not the gold's is counted as an abstention or as a different confident answer.
The direction is the point. An abstention is jevify saying it does not know; a confident wrong
answer is jevify being wrong without saying so.

The gold is read here and only here, from `evals/holdout/gold/gold.jsonl`.
"""
import argparse
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
HOLDOUT = REPO / "evals" / "holdout"
ABSTAIN = {"none", "unsure", "ambiguous"}


def load_gold():
    """The gold, with a `pick` line number resolved to the record's own text so that it can be
    compared with an answer that came back from a reshuffled list."""
    cases = {json.loads(ln)["id"]: json.loads(ln)
             for ln in (HOLDOUT / "cases.jsonl").read_text().splitlines() if ln.strip()}
    gold = {}
    for ln in (HOLDOUT / "gold" / "gold.jsonl").read_text().splitlines():
        if not ln.strip():
            continue
        row = json.loads(ln)
        answer, case = row["gold"], cases.get(row["id"], {})
        if case.get("verb") == "pick" and isinstance(answer, str) and answer.isdigit():
            records = (HOLDOUT / case["stdin"]).read_text().splitlines()
            answer = records[int(answer) - 1]
        gold[row["id"]] = answer
    return gold


def verdict(gold, answer):
    """`right`, `abstain` or `wrong` for one answer against the gold."""
    if answer == "not_run":
        return "not_run"
    if isinstance(gold, dict) and "any_of" in gold:
        accepted = [str(a) for a in gold["any_of"]]
        return "right" if answer in accepted else ("abstain" if answer in ABSTAIN else "wrong")
    if isinstance(gold, list) and len(gold) == 2 and all(isinstance(x, int) for x in gold):
        if answer in ABSTAIN:
            return "abstain"
        return "right" if answer.isdigit() and gold[0] <= int(answer) <= gold[1] else "wrong"
    if isinstance(gold, str) and gold in ABSTAIN:
        return "right" if answer in ABSTAIN else "wrong"
    if answer in ABSTAIN:
        return "abstain"
    return "right" if answer == str(gold) else "wrong"


def read(paths):
    rows = []
    for path in paths:
        for ln in Path(path).read_text().splitlines():
            if ln.strip():
                rows.append(json.loads(ln))
    return rows


def group(rows):
    """{(backend, verb, id): {index: answer}}"""
    out = defaultdict(dict)
    for row in rows:
        out[(row["backend"], row["verb"], row["id"])][row["index"]] = row["answer"]
    return out


def baseline_of(answers_by_index):
    """The question's settled answer: the one most of its runs give, and on a tie the one the
    input's own order gives."""
    counts = Counter(answers_by_index.values())
    top = max(counts.values())
    tied = [a for a, n in counts.items() if n == top]
    first = answers_by_index.get(min(answers_by_index))
    return first if first in tied else sorted(tied)[0]


def drift(answers_by_index, gold):
    """How far one question's answers move off its own settled answer, and in which direction.

    A run that answers something else has either stopped answering — an abstention, which says
    so, and which the caller sees as exit 3 — or answered a different thing with the same
    confidence, which does not say so at all. The two are counted apart, and only where the
    settled answer was the gold's: a question whose settled answer already abstains or is
    already wrong has nothing to degrade from, and its changes are counted on their own.
    """
    base = baseline_of(answers_by_index)
    settled = verdict(gold, base)
    cell = Counter(questions=1, runs=len(answers_by_index))
    cell["settled_" + settled] += 1
    for answer in answers_by_index.values():
        if answer == base:
            continue
        cell["changed"] += 1
        if settled != "right":
            cell["off_unsettled"] += 1
        elif answer in ABSTAIN:
            cell["right_to_abstain"] += 1
        else:
            cell["right_to_other"] += 1
            if verdict(gold, answer) == "wrong":
                cell["right_to_wrong"] += 1
    if cell["changed"]:
        cell["unstable"] += 1
    return cell


def table(per, unit):
    """One line per backend and verb, and a total under it."""
    print(f"{'backend':<11} {'verb':<7} {'questions':>9} {unit:>7} {'unstable':>9} "
          f"{'change rate':>12} {'right->abstain':>15} {'right->other':>13} {'of those wrong':>15}")
    total = Counter()
    for (backend, verb), cell in sorted(per.items()):
        total.update(cell)
        print(f"{backend:<11} {verb:<7} {cell['questions']:>9} {cell['runs']:>7} "
              f"{cell['unstable']:>9} {cell['changed'] / cell['runs']:>11.1%} "
              f"{cell['right_to_abstain']:>15} {cell['right_to_other']:>13} "
              f"{cell['right_to_wrong']:>15}")
    print(f"{'all':<11} {'all':<7} {total['questions']:>9} {total['runs']:>7} "
          f"{total['unstable']:>9} {total['changed'] / total['runs']:>11.1%} "
          f"{total['right_to_abstain']:>15} {total['right_to_other']:>13} "
          f"{total['right_to_wrong']:>15}")
    print(f"\nsettled answers: {total['settled_right']} right, "
          f"{total['settled_abstain']} abstaining, {total['settled_wrong']} wrong; "
          f"{total['off_unsettled']} changes move off an answer that was not the gold's.")
    return total


def score_repeat(rows):
    gold = load_gold()
    grouped = group(rows)
    per = defaultdict(Counter)
    for (backend, verb, cid), trials in grouped.items():
        per[(backend, verb)].update(drift(trials, gold.get(cid)))
    table(per, "runs")
    kept = defaultdict(lambda: defaultdict(set))
    for row in rows:
        if row["verb"] == "filter" and row["answer"] == "keep":
            kept[(row["backend"], row["unit"])][row["index"]].add(row["id"])
    if kept:
        print(f"\n{'backend':<11} {'filter run':<12} {'kept per run':<22} {'union':>6} "
              f"{'intersect':>10} {'unstable':>9}")
        for (backend, unit), runs in sorted(kept.items()):
            sets = [runs[i] for i in sorted(runs)]
            union, inter = set().union(*sets), set.intersection(*sets)
            print(f"{backend:<11} {unit:<12} {str([len(s) for s in sets]):<22} {len(union):>6} "
                  f"{len(inter):>10} {len(union) - len(inter):>9}")
    not_run = sum(1 for r in rows if r["answer"] == "not_run")
    print(f"\n{len(rows)} answers, {not_run} not_run, "
          f"{sum(r['questions'] for r in rows)} semantic questions")


def score_order(rows):
    gold = load_gold()
    grouped = group(rows)
    per = defaultdict(Counter)
    for (backend, verb, cid), orders in grouped.items():
        per[(backend, verb)].update(drift(orders, gold.get(cid)))
    total = table(per, "orders")
    degraded = total["right_to_abstain"] + total["right_to_other"]
    print(f"{total['runs']} (question, order) pairs over {total['questions']} questions: "
          f"{total['unstable']} questions do not give the same answer under all their orders.")
    if degraded:
        print(f"{degraded} answers degrade off a settled right answer: "
              f"{total['right_to_abstain']} stop answering "
              f"({total['right_to_abstain'] / degraded:.0%}), {total['right_to_other']} answer "
              f"something else with the same confidence "
              f"({total['right_to_other'] / degraded:.0%}), of which "
              f"{total['right_to_wrong']} are wrong against the gold.")
    print(f"{sum(r['questions'] for r in rows)} semantic questions")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("arm", choices=["repeat", "order"])
    parser.add_argument("runs", nargs="+")
    args = parser.parse_args()
    rows = read(args.runs)
    (score_repeat if args.arm == "repeat" else score_order)(rows)
    return 0


if __name__ == "__main__":
    sys.exit(main())

# How far the answer moves when the question does not

Measured on jevify 0.11.0 at `ca75d99`, with `JEVIFY_NO_CACHE=1` on every call, over the fixed
subset of `evals/holdout/` that `variance_run.py` names. Both backends answer with `jev-1.13.0`
at the default threshold of 0.5. The binary is a release build of that commit with no
uncommitted source change in it. Every call in both arms completed: no `not_run`, no HTTP 402,
no HTTP 502.

```sh
python3 evals/variance/variance_run.py repeat --backend <backend> \
    --out evals/out/variance/repeat-<backend>.jsonl
python3 evals/variance/variance_run.py order --backend <backend> \
    --out evals/out/variance/order-<backend>.jsonl
python3 evals/variance/variance_score.py repeat \
    evals/out/variance/repeat-classifier.jsonl evals/out/variance/repeat-typesafe.jsonl
python3 evals/variance/variance_score.py order \
    evals/out/variance/order-classifier.jsonl evals/out/variance/order-typesafe.jsonl
```

## 1. Cold repeatability: the same question, five runs

43 questions per backend — 20 `filter` records over two runs, 7 `pick`, 6 `why`, 6 `route`,
4 `fill` — each asked five times with the cache off and nothing else changed.

| Backend | Verb | Questions | Answers | Questions that move | Answers off the settled one |
|:---|:---|---:|---:|---:|---:|
| classifier | `fill` | 4 | 20 | 0 | 0.0% |
| classifier | `filter` | 20 | 100 | 1 | 2.0% |
| classifier | `pick` | 7 | 35 | 0 | 0.0% |
| classifier | `route` | 6 | 30 | 0 | 0.0% |
| classifier | `why` | 6 | 30 | 1 | 3.3% |
| typesafe | `fill` | 4 | 20 | 0 | 0.0% |
| typesafe | `filter` | 20 | 100 | 2 | 3.0% |
| typesafe | `pick` | 7 | 35 | 0 | 0.0% |
| typesafe | `route` | 6 | 30 | 1 | 3.3% |
| typesafe | `why` | 6 | 30 | 0 | 0.0% |
| **both** | **all** | **86** | **430** | **5** | **1.6%** |

Five of the 86 questions do not give the same answer five times: two `filter` records on
TypeSafe, one on the keyless backend, one `route` question and one `why` question.

Of the six answers that differ from their question's settled answer, five are abstentions —
`unsure` where the settled answer is `keep`, `none` where the settled answer is a tool. The
sixth is `why-c-link-undefined` moving from line 1 to line 4 of the same log, and the gold for
that case is the block `[1, 4]`, so both lines are the root cause. **No cold rerun turns a right
answer into a different confident answer.**

### The kept set of a `filter` run

The union of what five runs keep, against the intersection:

| Backend | Run | Kept per run | Union | Intersection |
|:---|:---|:---|---:|---:|
| classifier | `filter-jst` | 5, 5, 5, 5, 5 | 5 | 5 |
| classifier | `filter-zox` | 4, 4, 3, 4, 3 | 4 | 3 |
| typesafe | `filter-jst` | 5, 5, 5, 5, 5 | 5 | 5 |
| typesafe | `filter-zox` | 3, 3, 4, 4, 4 | 4 | 3 |

One record of ten sits between the union and the intersection on both backends, and it is the
same record: a run keeps it or says `unsure` about it, never drops it. A pipeline that reruns a
cold `filter` gets a set that is stable to within one record in ten here, and the instability is
a record entering or leaving through `unsure`, not through `drop`.

## 2. Order sensitivity: the same candidates, eight orders

29 questions per backend — 20 `filter` records, 4 `pick`, 3 `route`, 2 `fill` — each asked under
the order its input file has and seven shuffles seeded by the case, so the eight orders are the
same eight orders on any machine. A `why` log is not a candidate list and takes no part.

| Backend | Verb | Questions | Answers | Questions that move | Answers off the settled one |
|:---|:---|---:|---:|---:|---:|
| classifier | `fill` | 2 | 16 | 0 | 0.0% |
| classifier | `filter` | 20 | 160 | 4 | 3.1% |
| classifier | `pick` | 4 | 32 | 1 | 3.1% |
| classifier | `route` | 3 | 24 | 1 | 12.5% |
| typesafe | `fill` | 2 | 16 | 0 | 0.0% |
| typesafe | `filter` | 20 | 160 | 7 | 6.2% |
| typesafe | `pick` | 4 | 32 | 1 | 3.1% |
| typesafe | `route` | 3 | 24 | 1 | 12.5% |
| **both** | **all** | **58** | **464** | **15** | **5.0%** |

15 of the 58 questions — 26 percent — do not give the same answer under all eight orders, on
byte-identical content. Order is the single largest source of movement measured here, larger
than rerunning the same order cold.

### Which direction the change takes

Of the 23 answers that differ from their question's settled answer, 14 start from a settled
answer that is the gold's and are therefore degradations:

| Direction | Count | Share of the degradations |
|:---|---:|---:|
| right → abstains (`none`, `unsure`) | 12 | 86% |
| right → a different confident answer | 2 | 14% |
| …of which wrong against the gold | 2 | 14% |

The other nine changes move off an answer that was not the gold's: a `route` question whose
settled answer is `none` answers `ffprobe` correctly under three of its eight orders, and a
`filter` record whose settled `keep` is wrong says `unsure` under one order. Reshuffling
recovers a right answer about as often as it loses one.

The two confident wrong answers are one question, `pick-zox-03`, under one order, on **both**
backends: the record `src/import/fasd.rs` is picked under seven orders and `src/db/mod.rs` under
the eighth, at full confidence both times. The two backends fail on the same shuffle. Order
sensitivity is a property of how the candidate list is put to the model, not of who answers it,
so a caller cannot route around it by changing backend.

`fill` reading a frozen candidate file does not move under any order on either backend, 16
answers each.

## What a harness author has to know

- The answer cache hides all of this. A repeat inside the cache window replays the first answer
  byte for byte; the variance is only ever visible cold, which is exactly the case of two
  workers racing the same query in a fleet, or of a cache that has just expired.
- Movement is roughly one answer in 60 cold and one in 20 across orders, and one question in
  four does not survive all eight orders. Treat a `filter` kept set as stable to within about a
  record in ten, not exactly reproducible.
- The direction is mostly honest. 12 of the 14 degradations stop answering, and jevify says so:
  exit 3 and `unsure` or `none`. A harness that retries an exit 3, widens the question, or hands
  it to a person loses nothing to this.
- The remaining direction is not honest, and it is the one to design against: the same question
  under a different candidate order answers a different record at full confidence, with nothing
  in the envelope marking it. Where the order comes from a lister — `git branch`, `ls`, a find —
  it is not chosen and not stable, so a harness that must not act on a wrong handle confirms the
  handle against the world before acting on it rather than trusting a single confident answer.
- The gate scores do not bound any of this. The contract claims no calibration, and the answer
  that changed under a reshuffle was reported at the same probability as the answer that did
  not. A score is the model's own number, not a bound on the spread of reruns.

## What this cost

About 4,100 semantic questions in total against the two backends: 1,712 for the two measurements
reported above — 960 for the repeatability arm, 752 for the order arm — and the rest for a first
pass whose record-to-ordinal mapping read a dropped record as unrun and which was rerun rather
than patched, plus the probes that built the runner. Nothing was billed: the keyless backend reports
`free_service` and TypeSafe reports no cost for Jev. The runs are evenly split between the two
backends, so about 2,045 went to the keyless quota.

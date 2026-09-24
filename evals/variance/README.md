# The variance set

How far a jevify answer moves when the question does not. Two measurements over a fixed subset
of `evals/holdout/`, each run with the answer cache off so that every answer is a live one:

- **Cold repeatability.** The same question, the same bytes, five runs. How often the five
  answers are not all the same, and for `filter` how far the kept set of one run is from the
  kept set of another.
- **Order sensitivity.** The same candidates under eight orders: the order the input file has,
  then seven shuffles. How often the answer changes, and which way the change goes.

The direction of a change is the point. A run that stops answering has said so — the exit code
is 3 and the caller can retry, widen or ask a person. A run that answers a different thing with
the same confidence has not.

## The files

| File | What it holds |
|:---|:---|
| `variance_run.py` | runs one arm against one backend and writes one JSON object per answer |
| `variance_score.py` | reads those runs, reads the gold, and reports per verb and backend |
| `FINDING.md` | the numbers, the version they were measured on, and what they cost |

The subset is named in `variance_run.py`: `REPEAT_SUBSET` is 25 units spanning all five verbs,
`ORDER_SUBSET` is the 11 of them whose candidates arrive as a list and therefore have an order.
A `why` log is not a candidate list, so `why` takes part in the repeatability arm only.

## Identity, not position

An answer has to mean the same thing under two different orders before two orders can be
compared, so every answer is recorded by an identity that a reshuffle does not move:

| Verb | What comes back | The identity recorded |
|:---|:---|:---|
| `pick` | the 1-based line of the chosen record | that record's own text |
| `filter` | a verdict per ordinal | the verdict, keyed by the record's own text |
| `fill` (`-` kind) | the handle | the handle |
| `route` | the tool name | the tool name |
| `why` | the 1-based line of the root cause | the line, on a log that is never permuted |

A shuffle is seeded by the unit's name and the order number, so the eight orders of a unit are
the same eight orders on every machine. Order 0 reads the frozen input itself; orders 1 to 7
read a scratch copy written beside the run.

## What is measured

A question's **settled answer** is the one most of its runs give, and on a tie the one the
input's own order gives. A run that answers something else has **changed**, and the change is
either an abstention — `none` for `fill`, `pick`, `why` and `route`, `unsure` for `filter` — or
a different confident answer. The gold of `evals/holdout/` says whether that different confident
answer is wrong; `variance_score.py` is the only side that reads it.

The scores at the gate do not bound any of this. The contract claims no calibration, and a
question answered at 0.9 in one run and abstained on in the next was 0.9 in the run that
answered it.

## Running it

```sh
cargo build --release
python3 evals/variance/variance_run.py repeat --backend classifier \
    --out evals/out/variance/repeat-classifier.jsonl
python3 evals/variance/variance_run.py order --backend classifier \
    --out evals/out/variance/order-classifier.jsonl
python3 evals/variance/variance_score.py repeat \
    evals/out/variance/repeat-classifier.jsonl evals/out/variance/repeat-typesafe.jsonl
python3 evals/variance/variance_score.py order \
    evals/out/variance/order-classifier.jsonl evals/out/variance/order-typesafe.jsonl
```

The scorer reads the run files it is given and nothing else, so it is handed the two backends'
runs by name: a glob over the output directory also catches whatever narrower runs `--unit` has
left there, and a stale answer scored beside a fresh one is not a measurement.

`--backend typesafe` reads the key from `TYPESAFE_API_KEY_FILE` in the environment; the runner
never opens that file. The keyless backend runs with both key variables removed. `--unit`
narrows a run to named units. Every call runs with `JEVIFY_NO_CACHE=1`, and each raw envelope is
kept under `<out dir>/raw/<arm>-<backend>/`.

A full pass is 25 units × 5 runs plus 11 units × 8 orders on each backend. `filter` answers ten
records in one call, so the call count is lower than the answer count and the semantic questions
of a `filter` call are counted once, not once per record.

## Provenance and licence

The subset is drawn from `evals/holdout/` and adds no material of its own; the sources, pins and
licences are the ones `evals/holdout/README.md` lists. The permuted copies are reorderings of
those frozen inputs and are written outside the tracked tree.

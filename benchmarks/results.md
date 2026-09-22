| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `pick cold` | 741.7 ± 40.4 | 692.4 | 838.4 | 223.02 ± 25.10 |
| `pick warm` | 6.8 ± 2.0 | 5.6 | 13.6 | 2.05 ± 0.65 |
| `is cold` | 451.0 ± 29.4 | 377.5 | 494.2 | 135.62 ± 16.01 |
| `why cold` | 644.6 ± 52.7 | 585.1 | 792.2 | 193.82 ± 24.81 |
| `run cold route-only` | 1849.4 ± 80.2 | 1756.6 | 2027.7 | 556.10 ± 59.85 |
| `run cold full` | 1984.5 ± 129.4 | 1864.3 | 2325.3 | 596.74 ± 70.48 |
| `rg baseline` | 3.3 ± 0.3 | 2.9 | 4.0 | 1.00 |

| Run | p50 [ms] | p95 [ms] | runs | failed |
|:---|---:|---:|---:|---:|
| `pick cold` | 739 | 804 | 15 | 0 |
| `pick warm` | 6 | 10 | 15 | 0 |
| `is cold` | 454 | 488 | 15 | 0 |
| `why cold` | 638 | 732 | 15 | 0 |
| `run cold route-only` | 1823 | 2007 | 15 | 0 |
| `run cold full` | 1940 | 2251 | 15 | 0 |
| `rg baseline` | 3 | 4 | 15 | 0 |

## Keyless quota per verb (measured 2026-09-22)

jevify 0.7.0, backend classifier.dev without a key, one IP, `JEVIFY_CONCURRENCY=4`,
`JEVIFY_NO_CACHE=1`. Requests went through a counting proxy on 127.0.0.1 that forwards them
unchanged and records what jevify sent (`items`, `dimensions`) and what the service accounted
(`usage.classifications`, `RateLimit-Remaining`). jevify's own count is
`meta.telemetry.semantic_questions`.

The service's unit is one classification: one item under one dimension, so a request of
`items × dimensions`. Its policy header is `RateLimit-Policy: 3000;w=60, 20000;w=86400`, per IP,
and `RateLimit-Remaining` drops by exactly `usage.classifications` (2990 → 2970 → 2920 for
batches of 20 and 50). No header reports the day's remainder. On that day the service answered
with `inclusionai/ling-3.0-flash` (`usage.fallback`), not Jev, without scores, so jevify refused
every answer (exit 4 `api_protocol`) and no verb reached its second round; the requests were sent
and counted all the same. A window of 100 labels answered HTTP 502 `upstream_other` on most
attempts and was not counted.

| Verb | Input | Requests sent | jevify `semantic_questions` | Service `classifications` | Result |
|:---|:---|---:|---:|---:|:---|
| `is` | 1 statement | 1 | 1 | 1 | counted |
| `is` | 3 statements | 1 | 3 | 3 | counted |
| `filter` | 10, 20, 50, 60 records | 1 each | 10, 20, 50, 60 | 10, 20, 50, 60 | counted |
| `filter` | 75, 99, 100 records | 1 each | 75, 99, 100 | 0 | HTTP 402 `request_spending_limit` |
| `label` (3 labels) | 100 records | 1 | 100 | 0 | HTTP 402 `request_spending_limit` |
| `pick` | 50 lines | 1 | 2 | 2 | counted (choice over 51 labels + `any`) |
| `pick` | 156 lines (2 windows) | 8 (with retries) | 4 | 0 | HTTP 502 on every attempt |
| `why` | 156 lines (2 windows) | 4 | 4 | 4 | the 58-label window counted twice, the 100-label one HTTP 502 |
| `route` | PATH of 1,883 commands (20 windows) | 6 | 40 | 2 | one window counted, the others HTTP 502 |
| `route` | inventory of 40 tools | 4 | 2 | 0 | HTTP 502 on every attempt |

Round 2 of `pick`, `why` and `route`: NOT RUN (no round 1 completed). Its shape is one request:
one item with one choice and one `any` question for `pick` and `why` (2 classifications), one
item with one question per finalist, at most 12, for `route`. `fill` uses the shape of `pick`.
Total counted by the service over the session: 154 classifications.

Cost per call, from the shapes and the counts above:

| Verb | Classifications per call | Calls a day on one IP (20,000) |
|:---|:---|:---|
| `is` | 1 per statement | 6,600 (3 statements) to 20,000 (1) |
| `filter`, `label` | 1 per distinct record; a batch of 60 records is accepted, 75 is refused (HTTP 402) | 20,000 records in total: 333 calls of 60 records, 2,000 calls of 10 |
| `pick`, `why` | 2 per window of 99 lines, plus 2 for the final round when there is more than one window | 10,000 calls of at most 99 lines; 830 of 1,000 lines; 100 of 9,801 |
| `route` | 2 per window of 99 commands, plus 1 per finalist (at most 12) | 380 over a PATH of 1,883 commands; 340 over 2,200 |

Routing does not cost one classification per command on the PATH: a window of 99 commands is
one item under two dimensions. Twenty routing calls over 2,200 commands cost about 1,200
classifications, well under 20,000; the 429 of 2026-09-19 is not explained by this count.

## The frozen validation set on both backends (measured 2026-09-22)

jevify 0.8.3 (commit 38b8b5c), the release binary, `JEVIFY_NO_CACHE=1`, threshold 0.5, the
default `is`/`filter` band 0.15. The set is `evals/validation/` (153 cases, 21 families,
calibration and validation splits, gold held apart from the manifest; hunch-ric). Runner:
`scripts/validation_run.py`; scorer: `scripts/validation_gold.py score`. Runs and every raw
envelope are under `evals/out/validation/` (not committed). Every decision and score below is
read from `meta.decision` of the envelope, never parsed from a status line. Environment-pinned
cases ran inside shallow clones of the four named repositories at their pinned commits; every
`route` case had its required tools on the PATH of the runner (a macOS dev machine, 1,900
commands, `spit` and `json_pp` among them).

Backends: TypeSafe with a key (`requested` jev-1.13.0, `answering` jev-1.13.0 on all 153
cases) and classifier.dev without a key (`requested` null, `answering` jev-1.13.0 on all 153).
No case was `not_run` on either backend: no 402, 429 or 502, and no fallback model. The two
runs took 74 s and 118 s; TypeSafe: 350 requests, 857 semantic questions, 1.71 M input tokens,
USD 0.072 by the envelope's own estimate; classifier.dev: 587 requests, 1,331 semantic
questions (jevify's count; the service counts one classification per item under one
dimension, so the day's remainder of 20,000 dropped by about that much on one IP).

The scorer's columns: `n` cases, `cov` coverage (decided over scored), `acc` accuracy on the
decided cases, `abst` abstention rate, `false` the number of false actions (a decision that
differs from a concrete gold, or any decision where the gold is `none`, `ambiguous`,
`unsure` or `?`), `n/r` not run.

### Validation split (only ever scored; 80 cases)

| Verb | n | TypeSafe cov / acc / abst / false | classifier.dev cov / acc / abst / false |
|:---|---:|:---|:---|
| `fill` | 7 | 0.86 / 0.83 / 0.14 / 1 | 0.86 / 0.83 / 0.14 / 1 |
| `pick` | 6 | 0.83 / 1.00 / 0.17 / 0 | 0.83 / 1.00 / 0.17 / 0 |
| `pick --from` | 5 | 0.80 / 1.00 / 0.20 / 0 | 0.60 / 1.00 / 0.40 / 0 |
| `why` | 12 | 0.75 / 0.89 / 0.25 / 1 | 0.83 / 0.80 / 0.17 / 2 |
| `filter` | 20 | 1.00 / 0.80 / 0.00 / 4 | 1.00 / 0.80 / 0.00 / 4 |
| `label` | 16 | 0.94 / 0.80 / 0.06 / 3 | 0.88 / 0.86 / 0.12 / 2 |
| `is` | 8 | 1.00 / 0.88 / 0.00 / 1 | 1.00 / 0.88 / 0.00 / 1 |
| `route` | 6 | 1.00 / 0.67 / 0.00 / 2 | 1.00 / 0.83 / 0.00 / 1 |

### Calibration split (where a threshold may be chosen; 73 cases)

| Verb | n | TypeSafe cov / acc / abst / false | classifier.dev cov / acc / abst / false |
|:---|---:|:---|:---|
| `fill` | 5 | 0.60 / 1.00 / 0.40 / 0 | 0.60 / 0.67 / 0.40 / 1 |
| `pick` | 6 | 0.83 / 0.80 / 0.17 / 1 | 0.83 / 0.80 / 0.17 / 1 |
| `pick --from` | 4 | 0.75 / 1.00 / 0.25 / 0 | 0.75 / 1.00 / 0.25 / 0 |
| `why` | 9 | 0.89 / 0.88 / 0.11 / 1 | 0.89 / 0.88 / 0.11 / 1 |
| `filter` | 20 | 1.00 / 1.00 / 0.00 / 0 | 1.00 / 1.00 / 0.00 / 0 |
| `label` | 16 | 0.75 / 0.92 / 0.25 / 1 | 0.75 / 0.92 / 0.25 / 1 |
| `is` | 7 | 1.00 / 0.86 / 0.00 / 1 | 1.00 / 0.86 / 0.00 / 1 |
| `route` | 6 | 0.83 / 1.00 / 0.17 / 0 | 0.83 / 1.00 / 0.17 / 0 |

Every abstention on both splits and both backends was right (the gold was `none`,
`ambiguous`, `unsure` or `?`) except one: `pickfrom-val-04` on classifier.dev, best 0.26 against
NONE 0.26 for a gold of `docs` (TypeSafe scored the same candidate 0.80 and answered it).

### Where the false actions come from

The score at the gate is `any` (the Noul) for `is` and `filter`, and `best` (the top Choice
probability) for the selection verbs; `route` has no Noul and no NONE.

- **Gold that says "do not act" answered with a confident score, on both backends.** The four
  `filter-val-dav-x` records are `Merge branch 'pr-NNN'` subjects under "the change is a bug
  fix"; the gold is `unsure` (nothing in the record says), the model says no at 0.13–0.17
  (TypeSafe) and 0.00–0.01 (classifier.dev), far below the 0.35–0.65 band. The two `is` cases
  with `unsure` gold (a fact the document does not address) score 0.03–0.05 and 0.00: the
  model reads "not stated" as "no". `pick-cal-04` and `fill-val-07` (gold `ambiguous`: two
  candidates fit) are answered at 0.79/0.90 and 0.88/0.93 with the runner-up at 0.15/0.06 and
  0.01/0.03: the model prefers one of the two. These eight cases are 8 of the 16 false actions
  on each backend and no threshold or band moves them: their
  scores sit where the confident correct answers sit.
- **`label` on the ruff family**: three records whose gold is `?` (none of `bugfix`,
  `documentation`, `performance` fits a `[ty]` type-checker change) are labelled at 0.64–0.74
  (TypeSafe 3, classifier.dev 2). The correct labels of the same runs score 0.66 and up, so
  the two groups overlap.
- **`route`**: "translate this paragraph into Japanese" has gold `none`, written for a PATH
  without a translator; this machine has `spit` ("translate some text through a Large Language
  Model") and both backends chose it at 0.85 and 0.94. The gold is right for the set's answer
  space and wrong for this PATH; the case is environment-dependent in a way `requires` does
  not capture. "pretty-print a JSON file" went to `json_pp` on TypeSafe (0.93, `jq` third at
  0.87) and to `jq` on classifier.dev (1.00, `json_pp` at 0.99): `route` has no margin rule,
  so a near tie is decided by the order.
- **`why`**: `rust-analyzer-32135833797` points at line 248 on both backends (gold 286–288);
  `npm-04` points at line 11 (TypeSafe, 0.63) and line 123 (classifier.dev, 0.28) for a gold
  block at 258–269, and classifier.dev's 0.28 passed the gate only because NONE was 0.14. The
  passing svelte log is a false action on classifier.dev alone (line 4 at 0.65, NONE 0.20).

### The two backends against each other

Same answering model, same threshold, same inputs. The decisions differ on 9 of 153 cases
(`why` 5, `fill`, `label`, `pick --from`, `route` one each). The score at the gate differs
by a median of 0.01–0.04 per verb, TypeSafe higher, but by up to 0.54 on one case
(`pickfrom-val-04`: 0.80 against 0.26), 0.35 on `why`, 0.29 on `route` and 0.28 on `pick`.
The classifier.dev run needs 1.7 times the requests and 1.6 times the semantic questions for
the same set (windows of 99 instead of 200). A number read at the gate on one backend is not
the same number on the other: any claim about a probability is scoped to the backend that
produced it.

### What the calibration split supports

Nothing beyond the threshold in place. Per verb and backend the calibration split holds 4 to
20 cases and 0 or 1 false action; every alternative mapping would be fitted to at most one
event:

| Verb, backend | correct decisions: n, lowest score | false actions: scores |
|:---|:---|:---|
| `label`, TypeSafe | 11, 0.66 | 0.69 |
| `label`, classifier.dev | 11, 0.70 | 0.81 |
| `pick`, TypeSafe | 4, 0.99 | 0.79 |
| `pick`, classifier.dev | 4, 0.99 | 0.90 |
| `why`, TypeSafe | 7, 0.39 | 0.63 |
| `why`, classifier.dev | 7, 0.40 | 0.28 |
| `is`, both | 6, 0.01 / 0.00 (a correct `no`) | 0.03 / 0.00 (an `unsure` gold) |
| `fill`, classifier.dev | 2, 0.96 | 0.80 |

`label`, `why` and `is` interleave correct and false scores; `pick` and `fill` separate them
on one case each, which is not evidence. The calibration split has no `unsure` gold for
`filter` (both annotators decided all 20 records), so the four `filter` false actions of the
validation split have no calibration counterpart and the band stays where it is. The
validation split is scored once and is not used to choose anything.

### Not measured

Repeatability (each case ran once per backend; the score differences between backends above
include whatever run-to-run variation the service has). `add` and `sort` (no cases). The
route gold under a PATH that has a translator. The 30 abstention-gold cases are enough to see
that abstentions are right when they happen, not enough to bound the false-action rate on
"nothing fits" inputs per verb: 1 to 5 such cases per verb and split.

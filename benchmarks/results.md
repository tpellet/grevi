The two tables below are the `bench.sh` run of 2026-09-19. Their `run cold route-only` and
`run cold full` rows come from the `run` verb, which no longer exists; `bench.sh` emits
`route cold archive` and `route cold dvd` in their place.

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
| `pick` | 2 per window of 99 lines, plus 2 for the final round when there is more than one window (one window decides alone) | 10,000 calls of at most 99 lines; 830 of 1,000 lines; 100 of 9,801 |
| `why` | 2 per window of 99 lines, plus 2 for the final round on every log, a one-window log included | 5,000 calls of at most 99 lines; 830 of 1,000 lines; 100 of 9,801 |
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
`route` case had its required tools on the PATH of the runner (a macOS dev machine, 1,883
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
validation split is scored once and is not used to choose anything, with one exception: the
three-way `filter` wording below was chosen on the cases of both splits, so for `filter` the
validation split is spent and its figures are development-set figures.

### `route` with the tie margin (measured 2026-09-22)

jevify 0.9.2 with `TIE_MARGIN` 0.10 (the working tree after commit ed37bd6), the release
binary, `JEVIFY_NO_CACHE=1`, threshold 0.5, the 12 `route` cases of both splits on the same
runner as above; `scripts/validation_run.py --verb route`, scored with
`scripts/validation_gold.py score`. `jev-1.13.0` answered every case on both backends
(TypeSafe 384 semantic questions, classifier.dev 624). A runner-up above the threshold and
within 0.10 of the best is a tie, exit 3.

The count is large: 8 of the 24 decisions are ties, 4 on each backend, the same four cases on
both. Every tied runner-up sits 0.00 to 0.05 from the best, so the margin of 0.05 would have
tied the same eight; the cases have several fitting tools, not one tool and noise.

| Case | TypeSafe best, next, gap | classifier.dev best, next, gap | gold |
|:---|:---|:---|:---|
| list what is inside a zip file | zipinfo 0.94, unzip 0.91, 0.03 | zipinfo 0.96, unzip 0.95, 0.01 | any of unzip, zipinfo |
| measure how long a request to a URL takes | curl 0.71, hyperfine 0.71, 0.00 | curl 0.95, time 0.90, 0.05 | curl |
| look up the IP address a hostname resolves to | host 0.96, dig 0.95, 0.01 | host 1.00, nslookup 0.99, 0.01 | any of dig, host, nslookup |
| pretty-print a JSON file | jq 0.94, python3 0.94, 0.00 | jq 1.00, json-glib-format 0.98, 0.02 | jq |

Two of the four have a gold that accepts every tied tool, so a caller reading `ties` gets the
answer either way; the other two lose a correct decision to a tool that also does the task.
The widest gap of a decided case is 0.12 (unpack a tar.gz: tar 0.72, archiveutil 0.60 on
TypeSafe); the next widest is 0.19. Scored per split, both backends: validation coverage 0.50,
accuracy 0.67 on the decided cases, abstention 0.50, 1 false action (`spit` for the translation
case, as above); calibration coverage 0.67, accuracy 1.00, abstention 0.33, 0 false actions.

### `filter` asked three ways (measured 2026-09-22, development set)

Every figure in this section is a development-set figure. The three wordings compared here
were chosen on the 40 `filter` cases of both splits together, so the validation split is not
an independent check of the wording in place and its independence for `filter` is spent; a
fresh held-out set is the next step. "Zero false actions" below is the count on the set the
wording was chosen on, not a held-out result.

The run above asked `filter` a yes/no Noul, which reads "the record does not say" as a
confident no. `filter` asks a three-way Choice instead: the record says the statement holds,
the record says it does not hold, or the record does not say; a record is kept at
P(holds) ≥ 0.65, dropped at P(does not hold) ≥ 0.65 and unsure otherwise. Same binary
otherwise, same set, same backends, `JEVIFY_NO_CACHE=1`, the 40 `filter` cases of both
splits (the development set), one run per backend:

| Gold | n | TypeSafe, Noul | TypeSafe, three-way | classifier.dev, Noul | classifier.dev, three-way |
|:---|---:|:---|:---|:---|:---|
| keep | 17 | 17 keep | 17 keep | 17 keep | 17 keep |
| drop | 19 | 19 drop | 16 drop, 3 unsure | 19 drop | 13 drop, 6 unsure |
| unsure | 4 | 4 drop | 4 unsure | 4 drop | 4 unsure |

Scored on the development set: TypeSafe cov 0.82 / acc 1.00 / abst 0.17 / false 0;
classifier.dev 0.75 / 1.00 / 0.25 / 0 (Noul: 1.00 / 0.90 / 0.00 / 4 on both). The four
`Merge branch 'pr-NNN'` records score P(does not say) 0.96–0.99 (TypeSafe) and 1.00 (classifier.dev). The kept records score
P(holds) 0.79 and up (TypeSafe) and 0.81 and up (classifier.dev); the dropped ones
P(does not hold) 0.83 and up and 0.67 and up, with P(holds) at most 0.03 and 0.02. The new
abstentions are `Merge pull request #NNN from cli/<branch>` subjects under "the change is a
bug fix" (`filter-cal-cli-x-01`, `-02`, `-04`, adjudicated `drop` from one `drop` and one
`unsure` annotation) at P(does not say) 0.78–0.95 on TypeSafe, and on classifier.dev those
three plus `filter-cal-cli-x-03`, `filter-cal-cli-07` and `filter-val-ruff-04`, decided
records whose P(does not hold) stops at 0.31–0.60. Every abstention keeps its record, so the
cost of the change is 3 (TypeSafe) to 6 (classifier.dev) extra records to glance at over the
40 cases, 16% to 32% of the 19 gold drops, against four silent drops before it.

Two other wordings of the three options were measured on the same set before this one. One
named the third option only as "nothing in the record decides it either way" and sent 6
(TypeSafe) and 10 (classifier.dev) of the 19 `drop` golds into it, among them `Prioritize
HackerOne for vulnerability reports` under "the commit bumps a dependency version" at
P(does not say) 0.36 and 0.88. One said that a record describing something else "than what
the statement is about" is a no: it decided the set as the wording in place does, and scored
`assertion failed: left == right` under "reports a failed assertion" at P(holds) 0.64
(TypeSafe) and 0.05 (classifier.dev), an unsure and a no on the plainest yes there is. The
wording in place scores that record 0.77 and 0.67, so the keyless margin on it is 0.02 above
the mark. On a repeat of the first wording, TypeSafe decided every case the same and
classifier.dev moved two `drop` golds across the 0.65 mark (`filter-cal-cli-03`, `-05`), so
a keyless score near the mark is worth about one run's variation.

`is` keeps its Noul; its two `unsure` golds are not re-measured here.

### Not measured

Repeatability (each case ran once per backend; the score differences between backends above
include whatever run-to-run variation the service has). `add` and `sort` (no cases). The
route gold under a PATH that has a translator. The 30 abstention-gold cases are enough to see
that abstentions are right when they happen, not enough to bound the false-action rate on
"nothing fits" inputs per verb: 1 to 5 such cases per verb and split.

## The `fill` finals shortcut on a held-out content-phrase set (measured 2026-09-22)

A decisive names round skips the finals of a `file` or `dir` marker when the runner-up name
is out of play (`RIVAL_RATIO * (none + other names) <= best`). The rule was written against
phrases from this repository; this set is 33 content phrases nobody read while writing it
(`evals/fill/finals/`): 24 `file` and 9 `dir` cases over ripgrep `3fce3b5`, fzf `b1be3a8`
and bat `4987f76`, each scope at most 94 candidates so every case takes the single-window
path where the rule applies, gold from two annotators (32 of 33 agreed; one adjudicated
`ambiguous`), three abstention golds. Three binaries ran every case once on both backends,
`JEVIFY_NO_CACHE=1`, `--dry-run`, Jev 1.13.0 answering every request
(`scripts/eval_fill_finals.py`): `pre` is 233e946 (0.8.0, the commit before 76e723e: three
name finalists, a decisive names round skips the finals with no runner-up test), `head` is
e5048ff (0.9.3: every name with p > 0 reaches the finals, the runner-up rule), and `off` is
e5048ff with the shortcut removed, so a tier-two marker always runs its finals. `finals ran`
counts the cases with a second request; `abstentions (right)` counts the abstentions and
those whose gold was `none` or `ambiguous`.

| Binary | Backend | finals ran | coverage | accuracy | abstentions (right) | false actions | requests |
|:---|:---|---:|---:|---:|---:|---:|---:|
| pre 233e946 | classifier.dev | 18 | 0.82 | 0.85 | 6 (3) | 4 | 51 |
| head e5048ff | classifier.dev | 21 | 0.85 | 0.93 | 5 (3) | 2 | 54 |
| off (finals always) | classifier.dev | 33 | 0.76 | 0.96 | 8 (3) | 1 | 66 |
| pre 233e946 | TypeSafe | 15 | 0.88 | 0.86 | 4 (2) | 4 | 48 |
| head e5048ff | TypeSafe | 18 | 0.82 | 0.89 | 6 (3) | 3 | 51 |
| off (finals always) | TypeSafe | 33 | 0.76 | 0.92 | 8 (3) | 2 | 66 |

Against 233e946, the rule at `head` lowers the false actions (4 to 2 on classifier.dev, 4 to
3 on TypeSafe) and sends three more cases per backend to the finals, so the requests rise by
three. The runner-up test is what catches them: "the mapping that assigns a syntax to the
files git reads" wins the names round as `src/syntax_mapping.rs` at 0.60 with the field in
play, and the finals choose `50-git.toml`; at 233e946 the same names round decided alone and
was wrong.

The rule itself is measured by `head` against `off`: it fires on 12 cases on classifier.dev
and 15 on TypeSafe, saving one request each (18% and 23% of the requests the set costs with
the finals always on). On one of those cases per backend the one-sided names round is wrong
and the finals would have abstained: "wraps long lines at the terminal width" chose
`src/wrapping.rs` at 0.72 (names 0.72, next 0.03, NONE 0.12), a file that only declares the
`WrappingMode` enum while the wrapping is in `src/printer.rs`; "starts the pager and
negotiates its arguments" chose `src/pager.rs` at 0.66 (names 0.66, next 0.10, NONE 0.06),
which only picks the pager while `src/output.rs` starts it. Both are files named for the
concept the phrase describes and holding something else: the exact case the rule assumes a
one-sided names round rules out, and it does not. On the other side, "the homebrew
packaging" is right on names alone (`pkg/brew` 0.78 and 0.91) and abstains on both backends
when the finals read the directory. So the shortcut buys one request per fire at the price
of one confident wrong file per 12 to 15 fires, and gains one right directory the finals
would have lost; the finals always on end with the fewest false actions (1 and 2, both of
them cases the shortcut did not touch) and the lowest coverage (0.76).

The premise of the rule, that names alone cannot select a wrong file for a content phrase
when the runner-up is out of play, fails on this set at about one fire in thirteen, on both
backends, with a name decoy each time. The numbers do not support the bead's own criterion
either way: false actions fell against 233e946 and requests rose. What they say is a cost
trade, one request against a 7 to 8% false-action rate on the cases the rule decides alone,
and `fill` is the verb where a wrong selection reaches a command. That decision belongs to a
bead of its own (hunch-ng2 files it); nothing in `src/` changed for this measurement.

The remaining misses are the finals' own and the rule does not touch them: TypeSafe reads
"the parallel directory walker" as `crates/ignore/examples/walk.rs` (0.74, over
`src/walk.rs`) and "emits one JSON object per line" as `crates/printer/src/jsont.rs` (0.64,
the type definitions, over `json.rs`), on `head` and `off` alike; classifier.dev abstains on
both. Not measured: repeatability (one run per case and backend), lists wider than one
window, and phrases about names, which the rule was designed for and this set leaves out.

## The `fill` finals always on for `file` and `dir` (measured 2026-09-22)

The decision the section above filed: `file` and `dir` markers always run their finals, and
the shortcut (a decisive names round with the runner-up out of play decides alone) stays for
`branch` and `commit`, where no held-out set exists. The same 33 cases ran once per backend
under the same conditions as above (`--dry-run`, `JEVIFY_NO_CACHE=1`, Jev 1.13.0 answering
every request on both backends, the clones at their pins, `scripts/eval_fill_finals.py`),
with the binary `wka`: 0.9.3 at 168bd87 plus this change, which is the `off` binary of the
section above with the shortcut kept for `branch` and `commit`; every case here is `file`
or `dir`, so the two binaries take the same path on this set and `wka` against `off` is a
repeat run of the same rule.

| Binary | Backend | finals ran | coverage | accuracy | abstentions (right) | false actions | requests |
|:---|:---|---:|---:|---:|---:|---:|---:|
| wka (finals always) | classifier.dev | 33 | 0.73 | 0.96 | 9 (3) | 1 | 66 |
| wka (finals always) | TypeSafe | 33 | 0.85 | 0.89 | 5 (3) | 3 | 66 |

Requests are the same as `off` (66, one names round and one finals per case). The decisions
moved between the two runs of the same rule on 2 cases on classifier.dev and 6 on TypeSafe,
which is the repeatability the section above listed as not measured. On classifier.dev the
finals abstained on "reads the input and feeds it to the matcher" (`src/reader.go`, right on
`off` at 0.81; names 0.08 with NONE 0.79 this time) and everything else held: the one false
action is `src/pager.rs` for "starts the pager and negotiates its arguments" on both runs,
`src/wrapping.rs` abstains on both. On TypeSafe the finals decided four cases that `off`
abstained on, three of them right (`gitignore.rs` 0.56, `pkg/brew` 0.64, `src/ansi.go` 0.60)
and one wrong: `src/pager.rs` at 0.78 for the pager phrase, which the finals of the `off`
run had abstained on; and abstained on `crates/core/flags/defs.rs`, right on `off` at 0.50.
So the pager decoy is not a case the finals rule out: they choose it on classifier.dev on
both runs and on TypeSafe on one of two, from the names (0.54 to 0.69, next 0.09 to 0.23)
and an excerpt that picks the pager without starting it. The wrapping decoy the finals
refuse on every run so far. Against `head` (the shortcut on for every tier-two kind, section
above), the false actions on this set are 2 to 1 on classifier.dev and 3 to 3 on TypeSafe,
at 12 to 15 more requests; the one-run figures of the section above (1 and 2) were the
better end of what two runs show. Not measured: `branch` and `commit`, where the shortcut
stands.

## The `commit` kind over this repository's 248 commits (measured 2026-09-22)

jevify 0.9.3 built from `cd49bcf` with `cargo build --locked` (debug), answering model
`jev-1.13.0` on both backends, `JEVIFY_NO_CACHE=1`, `JEVIFY_DECISION=round_one`, every case
`--dry-run`. The set, the descriptions, the runs and the method are `evals/commit-attractor/`;
each description was written from its target's diff with `git show --format=`, which prints no
commit message, and no description was changed after a run.

| Set | Backend | cases | decided | right first | wrong | abstained | target in the finals | windows | requests/case |
|:---|:---|---:|---:|---:|---:|---:|---:|---:|---:|
| `commit` | classifier.dev | 27 | 13 | 11 | 2 | 14 | 21 | 3 | 4 |
| `commit` | TypeSafe | 27 | 19 | 14 | 5 | 8 | 23 | 2 | 3 |
| `branch` (control) | classifier.dev | 14 | 12 | 12 | 0 | 2 | — | 1 | 1 to 2 |
| `branch` (control) | TypeSafe | 14 | 12 | 12 | 0 | 2 | — | 1 | 1 to 2 |

No commit is an attractor. Every wrong answer is a different commit on both backends
(classifier.dev `dadfe99`, `1128d46`; TypeSafe `cb0547f`, `f0e3c91`, `62680bc`, `1128d46`,
`7855eb0`), and `332191a` is returned for exactly one description on each backend, the one
written from its own diff, at 0.74 and 0.91, held over three repeats (0.66, 0.62, 0.81 and
0.85, 0.85, 0.63).

Recall by the window the target falls in is 4 of 12, 7 of 10 and 0 of 5 on classifier.dev, and
14 of 22 and 0 of 5 on TypeSafe: the oldest window is 0 on both. Of the classifier.dev cases
whose target reached the finals, 11 are right, 9 abstain and 1 is wrong, so the keyless failure
is an abstention with the right commit in the finals, not a wrong commit.

Pool size against evidence, on the seven cases whose target is among the newest 60 commits, with
the evidence held to one `sha<TAB>subject` line per commit (the `-` kind, no finals round):
classifier.dev 2 of 7 at pool 60 and 2 of 7 at pool 248, TypeSafe 4 of 7 and 2 of 7. The same
seven cases under the `commit` kind, whose finals add the body and the changed paths, are 4 of 7
and 5 of 7. The finals round, not the pool size, is what makes the kind work.

A description fitting no commit: `ports the user interface to Android` abstains on both backends.
`rewrote everything in Go`, which `docs/demo/examples.sh` runs as its nothing-fits
demonstration, abstains on classifier.dev and returns `041e6d1` on TypeSafe in four runs of four,
at 0.72, 0.52, 0.71 and 0.73 against none 0.20 to 0.26.

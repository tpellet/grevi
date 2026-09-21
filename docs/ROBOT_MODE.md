# jevify — robot mode

jevify judges the output side of a command: existing records in, pointers or decisions out.
It selects and never generates. Use `jevify capabilities --json` as the source of truth;
`jevify init agents` prints an instruction block derived from its command and exit tables.

## When to call it

| Trigger | Verb | Classical twin |
|:---|:---|:---|
| a long failed build or a grep that found only the symptom | `why` | — |
| one record described but not named | `pick` | `fzf --filter` |
| many records or files, one question | `filter` | `grep` |
| the next step depends on a fact | `is` | `test` |
| an unfamiliar task on a large PATH | `route` | command discovery |
| tracked changes mixed across topics | `add` | `git add -p` |
| files that need a home among existing folders | `sort` | folder placement |

Cheap tools go first. Skip jevify when literal search answers the question, the input is short
enough to read, or the exact command is known. Use one `filter` process for many records, never
a loop of `is` calls. Write a literal statement about the evidence, not a vague request for advice.
No counting, arithmetic, date comparisons or quality judgments; English works best.

```sh
gh run view --log-failed | jevify why --json
git log --oneline | jevify pick --json 'the commit that renamed the project'
fd -0 -e txt | jevify filter -0 --files 'asks for a refund'
printf 'All tests passed.\n' | jevify is 'the tests passed' && printf 'ready\n'
jevify route --json 'keep my mac awake for an hour'
jevify add --json --dry-run 'the token expiry fix'
```

`jq`, `cut`, `grep`, `head` and another jevify verb can consume selected records. `pick` and
`filter` preserve their exact bytes and input order. Check a `pick` call's exit before using
its output as an argument; unchecked substitution can turn abstention into an empty argument.

## Verbs and data

- `pick '<intent>' [-n N] [--index | --files] [-0 | --para]` reads stdin records.
  Data: `matches[{line,text,ordinal,p,lossy?}]`, `any`, `source`. Exit 0 found, 3 nothing fits.
  `--files` is boolean: `git ls-files | jevify pick --files 'where man pages are parsed'`.
  Paths are ranked first, then eligible excerpts of at most 24 finalists. Input is not saved.
- `why [-C N] [-n N] [--no-save]` reads stdin logs and prints numbered causes with context;
  it takes no split option. Data: `causes[{line,text,p,context[]}]`, `any`, `considered`, `total`,
  `hint`, `saved_input`, `complete`. Exit 0 found, 3 abstain. Pipe stderr with `2>&1`.
- `filter '<statement>' [-v] [-c] [--strict] [-0 | --para] [--files] [--no-save]` keeps records.
  `-v` inverts, `-c` counts, unsure records stay unless `--strict`. `--verbose` has no short flag.
  Data: `records[{text,ordinal,p,verdict,lossy?}]`, `kept`, `total`, `unsure`, `complete`,
  `saved_input`, `excerpts_withheld`. Exit 0 kept some, 1 kept none, 3 every record unsure.
- `is '<statement>' ['<statement>' ...] [--context FILE] [--band 0.15]` reads one context.
  One statement prints nothing; several print `VERDICT<TAB>STATEMENT` (`yes`, `no`, `unsure`).
  Data for one: `p`, `verdict`, `truncated`; for several: `statements[{statement,verdict,p}]`,
  aggregate `verdict`, `truncated`. Oversized context adds a reason and null probabilities,
  with no inference. Exit 0 all yes, 1 any no, 3 otherwise.
- `route <intent...>` prints a tool, summary and synopsis; starts no user command and selects
  no arguments. Data: `tool`, `summary`, `synopsis`, `fit`, `alternatives[{tool,fit}]`.
  Exit 0 found, 3 nothing fits. Missing synopsis is null.
- `add '<topic>' [--dry-run | --yes]` scores tracked unstaged hunks. Data:
  `hunks[{file,header,p,staged}]`. Exit 0 scored or staged, 3 no match, 6 empty or oversized,
  130 declined. Machine mode stages only with `--yes`; never commits.
- `sort <DIR> [--into ROOT] [--apply | --undo LOG]` proposes existing folders.
  Data: `moves[{from,to,p}]`, `skipped[{file,reason}]`, `undo_log`, `applied`. Exit 0 success,
  3 nothing placed or restored, 6 input error. Moves require `--apply` or `--undo`; no prompt.
  Atomic no-replace moves and a unique JSONL recovery journal protect occupied destinations.
  Symlink entries are skipped; same volume only; concurrent source replacement unsupported.
- `capabilities` prints commands, flags, data fields, exit codes, environment and limits.
- `robot-docs [guide|commands|exit-codes|examples|privacy]` prints `topic` and `text` in machine mode.
- `health` reports `backend`, `base_url`, `key`, `api`, `latency_ms`, `models`; exit 0, 4 or 5.
- `init zsh|bash|agents` prints `script`. Shell integration routes through `jevify route`.

`pick` and `filter` split lines by default, NUL records with `-0`, paragraphs with `--para`.
They limit distinct records to 20,000 within 64 MiB. `filter` judges identical records once and
restores all occurrences. Non-UTF-8 machine records carry `text`, `lossy: true` and `ordinal`;
human output preserves exact bytes. `pick` also uses `line` for its 1-based input position.

`--files` paths remain candidates when excerpts are withheld. Hidden or secret-looking components
and symlink file entries receive no excerpt; status reports `excerpts withheld: N`. See
[Privacy](../PRIVACY.md) for the exact checks. Excerpts are not complete file evidence.

Only `why` and `filter` save raw inputs, secrets included, never pruned. The directory is
`JEVIFY_CACHE_DIR/outputs` or the platform cache directory's `jevify/outputs`.
`--no-save` is independent of `--no-cache`. Stderr names the saved file:
`jevify why: full output: PATH` or `jevify filter: kept N of M, U unsure, full output: PATH`.
A failed or skipped save reports `full output: not saved (REASON)`, with `saved_input=null`
and `complete=false`. Compare `why.considered` with `why.total` separately for selection coverage.

## Exit codes and recovery

Write the condition so that yes means act. `&&` acts only on exit 0; use `case` to distinguish
no, unsure and errors. Under `git bisect run`, map unsure exit 3 to 125.

| Code | Meaning / response |
|---:|:---|
| 0 | yes, found or successful operation |
| 1 | `is`: any no; `filter`: kept none |
| 2 | usage error: read the corrected command in `error.example` |
| 3 | nothing fits or unsure: inspect evidence; do not retry until it agrees |
| 4 | unavailable or quota exhausted: read the error |
| 5 | missing or rejected TypeSafe key |
| 6 | empty, oversized or unreadable input; `too_many`: narrow with `grep` or `head` |
| 7 | reserved |
| 130 | declined at `add` confirmation |

`rate_limit_day` HTTP 429 is exit 4, `daily quota of the free backend reached`, without retry.
Filter batch requests honour numeric `Retry-After` through 60 seconds, refusing longer waits.
A failed later batch can leave a human-output prefix; do not treat it as complete input coverage.

## Envelope and telemetry

`--json` (alias `--robot`) prints one envelope, including usage errors. There is no automatic
JSON switch for pipes. `--format jsonl` prints one line; `--format toon` encodes the same fields.

```text
{ok, command, version, exit_code, data,
 meta{backend, model, elapsed_ms, requests, cache_hits, input_tokens, cost_usd,
      threshold, request_id, telemetry}, error{kind, message, hint, example} | null}
```

Branch on `exit_code`, which equals the process exit code, then read `data`. Error kinds are
stable identifiers. `meta.model` is a string, several answering models joined with `", "`.
TypeSafe defaults to `jev-1.13.0`; classifier chooses its model and rejects explicit overrides.
`meta.request_id` names the last TypeSafe inference request when reported; `health` records none.

`meta.requests` counts attempted inference POSTs, including retries and failures, excluding
health and prewarm GETs. `meta.telemetry` separates `inference_posts`, `health_gets`,
`prewarm_gets` and `semantic_calls`. Each group obeys:

```text
attempted = succeeded + failed + cancelled + in_flight
```

An attempt starts immediately before a send, not while waiting for a concurrency permit.
Transport success means HTTP 200 with the complete body, so invalid decisions can succeed at
transport and fail semantically. Dropped futures count as cancelled; pending prewarm stays in
flight. Semantic accounting includes cache hits and locally rejected calls. `semantic_questions`
counts submitted questions. `logical_rounds` is null: HTTP accounting cannot infer stage counts.

`retry_sends` counts actual sends after the first attempt. `retry_sleep_ms` sums elapsed completed
or interrupted waits, excluding pending waits, HTTP time and semaphore waits.

`usage.input_tokens` and `usage.output_tokens` each contain `reported_subtotal`,
`reported_attempts`, `unknown_attempts` and `complete`. Only valid service-reported counts enter
subtotals; missing fields, malformed bodies, failed transport, cancellation and pending attempts
remain unknown. For each token field:

```text
reported_attempts + unknown_attempts = inference_posts.attempted
```

`meta.input_tokens` is null unless input usage is complete; unknown does not mean zero.
Cache hits add no inference attempt or service usage. `cost_estimate` supplies `basis`,
`input_price_per_mtok`, `reported_input_subtotal_usd` and `complete`. It estimates input tokens
only, not a billing receipt. `meta.cost_usd` is null when that basis is incomplete, except that
a configured zero price yields zero regardless of usage. Classifier defaults to `free_service`;
other pricing uses `configured_input_token_price`. No output-token price is invented.

## Permissions and judgment limits

Output verbs start no user command. The caller authorizes `add` staging and `sort` moves.
`is` abstains on oversized context; `add` rejects oversized hunks and complete batches before
staging. A higher threshold cannot validate missing evidence or grant permission.

`filter` batches up to 1,000 records on classifier.dev, each judged alone. On TypeSafe 20 records
share a request state and each question names its record; independence is not claimed.
`p` is a backend score. Calibration needs task- and backend-specific evidence; ranks past the
third are candidates without a reliability claim. Text can influence the model with embedded
instructions, so semantic judgments are not security gates.

Outbound secret masking is best effort. Answer cache keys use redacted requests, expire after
seven days and never cross backend, endpoint or decision-contract versions. Raw saved inputs
are a separate store. Neither telemetry nor diagnostics prints credentials.

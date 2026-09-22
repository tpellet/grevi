# jevify — robot mode

jevify works on two sides of a command: `fill` turns descriptions into real input arguments
and runs the command; output verbs turn existing records into pointers or decisions.
It selects and never generates. Use `jevify capabilities --json` as the source of truth;
`jevify init agents` prints an instruction block derived from its command and exit tables.

## When to call it

| Trigger | Verb | Classical twin |
|:---|:---|:---|
| about to list branches, commits, files, PRs or runs only to choose one, or a tool can list the needed value | `fill` | argument lookup |
| want the handle without executing | `pick --from KIND` | selection |
| an option depends on unread context | `fill` with `one` or `flag` | conditional arguments |
| a long failed build or a grep that found only the symptom | `why` | — |
| one record described but not named | `pick` | `fzf --filter` |
| many records or files, one question | `filter` | `grep` |
| every record needs a bucket | `label` | an `awk` key |
| the next step depends on a fact | `is` | `test` |
| an unfamiliar task on a large PATH | `route` | command discovery |
| tracked changes mixed across topics | `add` | `git add -p` |
| files that need a home among existing folders | `sort` | folder placement |

Cheap tools go first. Skip jevify when literal search answers the question, the input is short
enough to read, or the exact command is known. Use one `filter` or `label` process for many
records, never a loop of `is` calls. Write a literal statement about the evidence, not a vague
request for advice. No counting, arithmetic, date comparisons or quality judgments; English
works best.

```sh
gh run view --log-failed | jevify why --json
git log --oneline | jevify pick --json 'the commit that renamed the project'
fd -0 -e txt | jevify filter -0 --files 'asks for a refund'
gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c
printf 'All tests passed.\n' | jevify is 'the tests passed' && printf 'ready\n'
jevify route --json 'keep my mac awake for an hour'
jevify add --json --dry-run 'the token expiry fix'
```

`jq`, `cut`, `grep`, `head` and another jevify verb can consume selected records. `pick` and
`filter` preserve their exact bytes and input order; `label` prints `LABEL<TAB>RECORD` with the
record unchanged after the tab. Check a `pick` call's exit before using its output as an
argument; unchecked substitution can turn abstention into an empty argument.

## Verbs and data

- `fill [--dry-run] [-q] [--candidates FILE] [--context FILE] [--field N | --key KEY]
  [-0 | --para] -- COMMAND ARGS...` resolves every marker or runs nothing. Data:
  `argv` on successful dry run, `markers[{arg,kind,reason,handle,p,candidates,total,omitted}]`,
  `reason`. Machine formats require `--dry-run`. The command inherits the environment and
  directory, and owns output, signals and exit code. Consumed stdin becomes empty for it.
- `pick '<intent>' [-n N] [--index | --files] [-0 | --para]` reads stdin records.
  Data: `matches[{line,text,ordinal,p,lossy?}]`, `any`, `source`. Exit 0 found, 3 nothing fits.
  `--files` is boolean: `git ls-files | jevify pick --files 'where man pages are parsed'`.
  Paths are ranked first, then eligible excerpts of at most 24 finalists. Input is not saved.
  `pick --from KIND '<intent>' [-n N]` lists a kind's candidates (`branch`, `commit`, `file`,
  `dir`, `tool`, `pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod`, or a user
  recipe); plain `pick` uses stdin. It prints handles,
  starts no user command, and conflicts with `--files`, `--index`, `-0`, `--para`.
  Data adds `reason`, `candidates`, `total`, `omitted`, `windows`, `finalists_per_window`;
  its matches have `text`, `ordinal`, `p`, `lossy`, without `line`.
- `why [-C N] [-n N] [--no-save]` reads stdin logs and prints numbered causes with context;
  it takes no split option. Data: `causes[{line,text,p,context[]}]`, `any`, `considered`, `total`,
  `hint`, `saved_input`, `complete`. Exit 0 found, 3 abstain. Pipe stderr with `2>&1`.
- `filter '<statement>' [-v] [-c] [--strict] [-0 | --para] [--files] [--no-save]` keeps records.
  `-v` inverts, `-c` counts, unsure records stay unless `--strict`. `--verbose` has no short flag.
  Data: `records[{text,ordinal,p,verdict,lossy?}]`, `kept`, `total`, `unsure`, `complete`,
  `saved_input`, `excerpts_withheld`. Exit 0 kept some, 1 kept none, 3 every record unsure.
- `label a,b,c [-0 | --para] [--files]` tags every record with one of the labels and prints
  `LABEL<TAB>RECORD` in input order; `?` marks an unsure record. Labels: at least two, distinct,
  none empty, none `?` or `NONE`, at most the backend window (99 on classifier.dev, 200 on
  TypeSafe), else exit 2. Data: `records[{label,text,ordinal,p,lossy?}]`, `labelled`, `total`,
  `unsure`, `complete`, `excerpts_withheld`. Exit 0 labelled, 3 every record unsure. Saves nothing.
  Stderr: `jevify label: labelled N of M, U unsure`. `cut -f2-` gives line records back without
  their blank lines; with `-0` and `--para` the record follows the tab unchanged.
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

`pick`, `filter` and `label` split lines by default, NUL records with `-0`, paragraphs with
`--para`. They limit distinct records to 20,000 within 64 MiB. `filter` and `label` judge
identical records once and restore all occurrences. Non-UTF-8 machine records carry `text`, `lossy: true` and `ordinal`;
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

## The marker and kinds

```sh
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
jevify fill --dry-run -- git revert '@{commit:made folder moves atomic}'
jevify fill --dry-run -- cat 'src/@{file:parses the marker}'
printf 'retry_backoff\nparse_header\n' | jevify fill --dry-run -- cargo test '@{-:the retry test}'
printf 'A crash with no reproduction steps.\n' | jevify fill --dry-run -- printf '%s\n' \
  '@{one:bug|feature|docs:what kind of report is this}' '@{flag:--draft:the report lacks steps to reproduce}'
jevify pick --from branch 'the auth refactor'
```

Three families: existing things (`branch`, `commit`, `file`, `dir`, `tool`, and the recipes
`pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod`), caller-written options
(`one`, `flag`), supplied records (`-`). `capabilities.kinds` lists every kind with its exact
lister argv and its origin, `coded`, `shipped` or `user`; `-`, `tool`, `one` and `flag` have
empty lister arrays. `branch` and `commit` are newest first; `branch` folds remote twins.
`file` and `dir` take a literal prefix ending in `/`; a `file` finalist adds first lines, withheld
for the patterns in `capabilities.withheld`. A recipe kind is one JSON line in `kinds.jsonl`
under `JEVIFY_CONFIG_DIR` or the platform configuration directory: `kind`, `list`, `field` or
`key`, `ordered`; `capabilities.recipes` states the fields and the rules. jevify reads no recipe
from a repository, a user recipe cannot replace a shipped kind, and a bad file is exit 6
`recipe_invalid` with its line number. Every lister has one 20 s deadline; a missing or
unauthenticated tool is exit 6 `lister_failed` with its own text. `--field N` is a 1-based
whitespace field; `--key KEY` extracts a JSON handle while retaining the record as evidence.
The `fill` status line reads
`candidates N[ of M[, newest first]][, omitted K], windows W[, excerpts withheld: E]`.

Quote the whole marker argument with single quotes, including any prefix or suffix. Spell an
apostrophe `'\''`. Marker escapes are `\}`, `\:` and `\|`; `one` separates options with `|`.
`flag` must occupy a whole argument: yes keeps it, no removes it, unsure abstains.
`@@{word:` spells a literal `@{word:`. `'{user}@{host:>8}'` is an unknown kind, exit 2;
`'{user}@@{host:>8}'` is literal. Missing closing braces, unknown kinds and no marker are exit 2.
Do not send markers through another shell (`ssh`, `make`, `xargs`). Preview with `--dry-run`,
never `eval`. Never use `"$(jevify pick …)"` as a command argument.

stdin supplies candidates for `-` or context for `one`/`flag`, never both. Use `--candidates FILE`
or `--context FILE` for the other role; file inputs preserve the command's stdin. All markers
resolve against one snapshot; any abstention prevents the entire execution.

Let W be the backend window: 99 on classifier.dev, 200 on TypeSafe. `fill` keeps three finalists
per window and accepts F = W × floor(W / 3): 3,267 or 13,200 candidates. `pick` and `pick --from`
accept min(W × W, 20,000): 9,801 or 20,000. They keep three finalists per window when those fit
W, else two when those fit, else one, always by rank within each window. `one` accepts at most
W options; more is exit 2. Ordered kinds retain the newest candidates and report coverage;
unordered overflow is `too_many`. No probabilities from separate requests are compared.

## Exit codes and recovery

Write the condition so that yes means act. `&&` acts only on exit 0; use `case` to distinguish
no, unsure and errors. Under `git bisect run`, map unsure exit 3 to 125.

| Code | Meaning / response |
|---:|:---|
| 0 | yes, found or successful operation |
| 1 | `is`: any no; `filter`: kept none |
| 2 | usage error: read the corrected command in `error.example` |
| 3 | nothing fits or unsure: inspect evidence; do not retry until it agrees; `filter` and `label`: every record unsure |
| 4 | unavailable or quota exhausted: read the error |
| 5 | missing or rejected TypeSafe key |
| 6 | empty, oversized or unreadable input; `too_many`: narrow with `grep` or `head` |
| 7 | reserved |
| 130 | declined at `add` confirmation |

`rate_limit_day` HTTP 429 is exit 4, `daily quota of the free backend reached`, without retry.
For `fill`, exits 2–6 mean nothing ran; after `exec`, the command owns its exit code, including
2–6. A successful dry run exits 0. Stderr lines start with `jevify fill:`; `-q` keeps only
`not run:` lines. Read the execution status as well as the exit code.

Input errors use `error.kind`, exit 6: `stdin_is_tty`, `lister_failed`, `too_many`, `cannot_run`,
`recipe_invalid`. Run the named lister yourself for `lister_failed`; narrow with a prefix,
`grep`, `head` or a narrower pipe for `too_many`.

Abstention is separate: exit 3, `error: null`, `data.reason` for the first failed marker in
argv order, and `data.markers[].reason` for every marker. `no_match`: read candidates N of M;
`ambiguous`: read the two handles and write one; `unsure_flag`: write the flag or drop the
marker; `insufficient_evidence`: supply a complete context that fits. These are not error kinds.
`fill` requires every answer to come from Jev, even for a dry run. Otherwise exit 4,
`api_unavailable`, `answered by <model>, not Jev`. A missing model name is `unknown` in
`meta.model` and refuses too. Read the quota or model line before retrying exit 4.

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

Allow output verbs and `jevify fill --dry-run` freely. Allow `fill` per command prefix, such as
`jevify fill -- git switch:*`, exactly as the underlying command. jevify is not a permission
system. Output verbs start no user command. The caller authorizes `add` staging and `sort` moves.
`is` abstains on oversized context; `add` rejects oversized hunks and complete batches before
staging. A higher threshold cannot validate missing evidence or grant permission.

`filter` and `label` batch up to 1,000 records on classifier.dev, each judged alone. On TypeSafe
20 records share a request state and each question names its record; independence is not claimed.
`p` is a backend score. Calibration needs task- and backend-specific evidence; ranks past the
third are candidates without a reliability claim. Text can influence the model with embedded
instructions, so semantic judgments are not security gates.

Outbound secret masking is best effort. Answer cache keys use redacted requests, expire after
seven days and never cross backend, endpoint or decision-contract versions. Raw saved inputs
are a separate store. Neither telemetry nor diagnostics prints credentials.

# FAQ

## Do I need an API key?

No. Without a key, jevify uses the free backend at classifier.dev. Setting
`TYPESAFE_API_KEY_FILE=/path/to/key` or `TYPESAFE_API_KEY` selects TypeSafe and its quota.
`JEVIFY_BACKEND=typesafe|classifier` forces a backend; `jevify health` checks it.
The verbs and exit codes are shared, but scores are not assumed interchangeable.

## What does a call cost?

The classifier backend defaults to zero service cost. Token usage may be unavailable, which
is `null`, not a measured zero. TypeSafe cost is an input-token estimate at
`JEVIFY_PRICE_PER_MTOK` (default $0.042 per million input tokens). `meta.cost_usd` is `null`
when that basis is incomplete unless the configured price is zero. `--verbose` prints diagnostics.
Repeated identical questions can use cached answers for seven days.

## What leaves my machine, and what stays?

The active backend receives the statement and evidence required by the verb. `filter` and
`label` send records, and `label` its labels; `pick` sends selection candidates; `why` sends filtered log lines; `is` sends its
supported context; `route` sends tool names, summaries and man-page evidence. Redaction is
best effort. [Privacy](../../PRIVACY.md) covers every verb and the `--files` withholding rules.

Only `why` and `filter` save raw input, secrets included, under the cache directory's `outputs/`.
Those files are never pruned. `--no-save` skips them; `--no-cache` only disables the separate
answer cache. A failed or skipped save sets `data.complete=false`.

## Why does it exit 3?

Nothing fits or the evidence is unsure. `pick` abstains instead of returning a bad candidate;
`filter` keeps unsure records unless `--strict`, and exits 3 when every record is unsure.
`label` prints an unsure record with the label `?` and exits 3 when every record is unsure.
`why` can abstain when no cause fits; make sure stderr reaches it through `2>&1`.
`is` returns 3 when no statement is no and at least one is unsure, or the context is too large.
`fill` runs nothing if any marker abstains. Its reasons are `no_match`, `ambiguous`,
`unsure_flag` and `insufficient_evidence`. With `--dry-run --json`, `error` is null;
`data.reason` is the first failed marker in argv order and `data.markers[]` has every result.

Write the condition so that yes means act. `&&` acts only on exit 0. An explicit `case` can
distinguish a no (1), an abstention (3), and backend or input errors.

## Why do hidden files still appear with `--files`?

The caller supplies the paths on stdin. A hidden or secret-looking path remains a candidate,
but its content is withheld before reading an excerpt. The name can still reach the backend.
The status reports `excerpts withheld: N`; this is not a guarantee that every secret is detected.

## Does it run a command or invent flags?

`fill` starts the command you wrote, substituting existing handles or options you supplied.
`branch`, `commit`, `file`, `dir` and `tool` list what exists; `pr`, `issue`, `ci-run`, `stash`,
`process`, `container` and `pod` run the owning tool's listing; `-` selects supplied records;
`one` and `flag` judge context. It invents nothing. Preview with `--dry-run`, never `eval`;
allow execution exactly as the underlying command is allowed. Use `pick --from KIND` when you
want a handle without execution.

A recipe kind is one JSON line: the command that lists, and which field is the handle. Your
own kinds live in `kinds.jsonl` under `JEVIFY_CONFIG_DIR` or the platform configuration
directory, never in a repository, and cannot replace a shipped kind. `jevify capabilities --json`
lists every kind with its command. See [Kinds](kinds.md).

`route` prints an installed tool, summary and synopsis. It supplies no arguments and starts no
user command. `why`, `pick`, `filter`, `label` and `is` judge supplied text. `add` can stage tracked hunks;
`sort --apply` and `sort --undo` can move files. These mutations require the caller's authorization.

## Why does a marker fail before inference?

Quote the whole argument: `'@{branch:the auth refactor}'`. Unknown kinds, a missing closing
brace, and no marker are exit 2. A literal `@{word:` is `@@{word:`; a Python format string
`'{user}@{host:>8}'` is an unknown kind, so use `'{user}@@{host:>8}'`. Do not pass markers through
a second shell. stdin cannot serve both candidates and context; supply `--candidates FILE`
or `--context FILE` for the other role. A required terminal stdin is exit 6 `stdin_is_tty`.

`too_many` means narrow with a prefix, `grep`, `head` or a smaller pipe. `lister_failed` means
run the named lister yourself: the message carries the tool's own text, such as `gh` asking for
a login. `recipe_invalid` names the line of your `kinds.jsonl` that does not parse, or that
names a shipped kind. `ambiguous` means inspect the two handles and write one; `unsure_flag`
means write the flag or drop its marker. For `no_match`, inspect candidates N of M.

## Whose exit code does fill return?

Before execution, 2–6 means nothing ran. After execution, the command owns its exit code,
including 2–6. Stderr reports `exec` or `not run:` with the `jevify fill:` prefix. `-q` keeps
only `not run:` lines; a successful dry run exits 0. Machine output requires `--dry-run`.
Only Jev may authorize a resolution: a different model gives exit 4 `api_unavailable`,
`answered by <model>, not Jev`. Missing model names appear as `unknown` and refuse too.

## Can I trust `p`?

It is a backend score whose calibration needs evidence for the task and question type.
The [routing measurements](how-it-works.md#numbers) do not establish calibration for every verb.
Raising `-t` changes the decision policy, not the amount of evidence. `is --band` adjusts its
unsure band; `filter` uses a fixed 0.15 band. Neither is a security gate for untrusted text.

## Why is the model pinned?

TypeSafe defaults to `jev-1.13.0` to make the requested model explicit. `--model jev-latest`
selects a moving alias. Classifier controls its model and rejects explicit overrides.
`meta.model` reports answering models as one string, joined with `", "` if several answer.
A model or prompt change needs fresh calibration evidence.

## Why not a loop of `is` calls?

Use one `filter` process for many records or files, or one `label` process when every record
needs a bucket. Both judge identical records once and send batches of up to 1,000 records on
classifier.dev or 20 on TypeSafe. Only classifier records are
judged independently; TypeSafe records share state. `is` instead judges several statements
about one context, reading stdin or `--context FILE`.

## What does daily quota exhaustion mean?

A `rate_limit_day` HTTP 429 returns exit 4 with `daily quota of the free backend reached`.
jevify does not retry it. Wait for the service quota to renew or use your TypeSafe quota.
The free quota is 20,000 classifications a day per IP, one per record under one question:
20,000 `is` statements, 20,000 `filter` or `label` records (at most 60 per call), 10,000 `pick`
or `why` calls under 100 lines, or about 380 `route` calls over a PATH of 1,900 commands
(measured 2026-09-22, [benchmarks/results.md](../../benchmarks/results.md)).
If a streaming filter fails after printing records, those records are only an answered prefix.

## Does it work offline or in other languages?

New questions need the hosted API; valid cached answers can replay without inference.
English works best. Dense input such as hashes or CJK text may exceed a backend token budget
and return exit 6, `api_rejected_request`; narrow it first. Use code for counting, arithmetic,
dates and ordering, and literal search when the words already identify the answer.

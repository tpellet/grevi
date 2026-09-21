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

The active backend receives the statement and evidence required by the verb. `filter` sends
records; `pick` sends selection candidates; `why` sends filtered log lines; `is` sends its
supported context; `route` sends tool names, summaries and man-page evidence. Redaction is
best effort. [Privacy](../../PRIVACY.md) covers every verb and the `--files` withholding rules.

Only `why` and `filter` save raw input, secrets included, under the cache directory's `outputs/`.
Those files are never pruned. `--no-save` skips them; `--no-cache` only disables the separate
answer cache. A failed or skipped save sets `data.complete=false`.

## Why does it exit 3?

Nothing fits or the evidence is unsure. `pick` abstains instead of returning a bad candidate;
`filter` keeps unsure records unless `--strict`, and exits 3 when every record is unsure.
`why` can abstain when no cause fits; make sure stderr reaches it through `2>&1`.
`is` returns 3 when no statement is no and at least one is unsure, or the context is too large.

Write the condition so that yes means act. `&&` acts only on exit 0. An explicit `case` can
distinguish a no (1), an abstention (3), and backend or input errors.

## Why do hidden files still appear with `--files`?

The caller supplies the paths on stdin. A hidden or secret-looking path remains a candidate,
but its content is withheld before reading an excerpt. The name can still reach the backend.
The status reports `excerpts withheld: N`; this is not a guarantee that every secret is detected.

## Does it run a command or invent flags?

`route` prints an installed tool, summary and synopsis. It supplies no arguments and starts no
user command. `why`, `pick`, `filter` and `is` judge supplied text. `add` can stage tracked hunks;
`sort --apply` and `sort --undo` can move files. These mutations require the caller's authorization.

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

Use one `filter` process for many records or files. It judges identical records once and sends
batches of up to 1,000 records on classifier.dev or 20 on TypeSafe. Only classifier records are
judged independently; TypeSafe records share state. `is` instead judges several statements
about one context, reading stdin or `--context FILE`.

## What does daily quota exhaustion mean?

A `rate_limit_day` HTTP 429 returns exit 4 with `daily quota of the free backend reached`.
jevify does not retry it. Wait for the service quota to renew or use your TypeSafe quota.
If a streaming filter fails after printing records, those records are only an answered prefix.

## Does it work offline or in other languages?

New questions need the hosted API; valid cached answers can replay without inference.
English works best. Dense input such as hashes or CJK text may exceed a backend token budget
and return exit 6, `api_rejected_request`; narrow it first. Use code for counting, arithmetic,
dates and ordering, and literal search when the words already identify the answer.

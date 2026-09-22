# Labelling instructions

You are one of two independent annotators for a benchmark. You label what the *correct answer* is
for each item, by reading the item's input yourself. You are not running any tool and you must not
guess what a tool would answer: label what a competent engineer would say is right.

Each item is one JSON object on a line of the batch file. Fields:

- `id` — the item id; every answer line must repeat it exactly.
- `verb` — the kind of question (see below).
- `task` — the description, statement or intent to judge.
- `input_file` — an absolute path you read (records, a log, or a context document), when present.
- `candidate_listing` / `repository` — when the candidates are a repository listing, the listing
  file is the frozen candidate list; treat it as the complete set of candidates.
- `record`, `record_ordinal` — for `filter` and `label`, the single record you judge.
- `labels` — for `label`, the only labels allowed besides `?`.
- `answer_space` — the values your answer may take.

## What each verb's answer means

- `fill`, `pick`, `pick --from` — one candidate from the input or the listing. Answer with the
  handle the item asks for: a 1-based line number for `pick` over a record file, a path for a
  `file`/`dir` candidate, the short sha for a commit candidate. Answer `none` when no candidate
  fits the description, and `ambiguous` when two or more fit it equally well and nothing in the
  description separates them.
- `why` — the 1-based line number, in `input_file`, of the first line of the block a competent
  engineer would point to as the cause of the failure: the first diagnostic that names the actual
  problem (not a summary line such as `error: could not compile`, `FAILED tests/...`,
  `Process completed with exit code 1`, `make: *** Error 1`). Answer `none` when the log shows no
  failure at all, and `ambiguous` when several unrelated failures have equal claim to being *the*
  cause.
- `filter` — `keep` when the statement in `task` is true of `record`, `drop` when it is false,
  `unsure` when a careful reader cannot decide from the record alone.
- `label` — exactly one of `labels`, or `?` when none fits or two fit equally.
- `is` — `yes` when the statement is true of the document in `input_file`, `no` when it is false
  or the document contradicts it, `unsure` when the document does not settle it.
- `route` — the command name a competent engineer on a typical macOS/Linux developer machine would
  reach for, `none` when no ordinary installed command does this, `ambiguous` when two common
  commands fit equally.

## Rules

- Read the input. Do not answer from the wording of the task alone.
- Judge the text literally. Do not count, calculate, or assume facts the input does not state.
- `none`, `ambiguous` and `unsure` are real answers, not failures: use them when they are right.
- Do not discuss your labels with anyone; this is an independent pass.

## Output

Write one JSON object per line to the output path you were given, nothing else in the file:

    {"id": "<item id>", "label": "<answer>", "confidence": "high|medium|low", "why": "<at most 15 words>"}

One line per item of your batch, in the batch's order, no omissions.

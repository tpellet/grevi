# grevi — robot mode

grevi answers a question about text that already exists: your input, the installed tools, a man
page, the folders on disk. It selects and never generates, so you can check every answer.

## When to call it

You have `grep` and you can read files. Call grevi when those two run out:

- A long log, or a grep that found the symptom. `grep -iE "error|fail"` finds the line that says
  something failed. The line that says why often holds none of those words (an assertion's
  `left:`/`right:` values, "could not match actual sql"). `why` reads the whole log, thousands of
  lines included. Call it before you read a log of more than a few hundred lines.
- Many texts, one question. Loop `is` over them and read only the exit codes: one line per text,
  whatever its length. Then read the few that said yes or unsure.
- One item out of many, described and not named: a branch, a commit, a file, a process, a
  history line. `pick` matches by meaning, so the description and the line need no word in common.
- Part of a working tree. `git add -p` needs a terminal. `add` stages the hunks that belong to one
  topic and leaves the others.
- A task with no command you are sure of. `run --dry-run --no-args` searches every command on the
  PATH by what its man page says it does, and only proposes what is installed.
- Files whose names say nothing. `sort` reads the content and proposes an existing folder.

Skip grevi when a literal search answers the question, when the input is short enough to read, or
when you already know the exact command.

## How to phrase

- Write what must be true of the text, literally. The statement is judged word for word:
  `"the customer is about to stop being a customer"` works; `"this customer is about to leave"`
  also says yes to an employee who is leaving their company.
- Describe the thing, not what you will do with it: `"the line with the failing assertion"`.
- One question per call. English works best.
- grevi does not count, do arithmetic, compare dates or judge quality.

## Patterns

    for f in tickets/*.txt; do                                  # triage, read none of them
      grevi is "the customer is about to stop being a customer" < "$f" >/dev/null 2>&1
      echo "$f $?"                                              # 0 yes · 1 no · 3 unsure
    done
    gh run view --log-failed | grevi why --json                 # root cause of a CI run
    git show "$(git log --oneline | grevi pick "the commit that renamed the project" | cut -d' ' -f1)"
    grevi add --json --dry-run "the token expiry fix"           # scores first; --yes stages

Human stdout is plain text made for pipes (`pick` prints the line, `is` prints nothing), so there
is no automatic switch to JSON when piped. Pass `--json` when you want the envelope.

## The envelope

Start here for the contract: `grevi capabilities --json`. Every command accepts `--json` (alias `--robot`) or
`--format json|jsonl|toon` and then prints exactly one envelope on stdout, usage errors included:

    { ok, command, version, exit_code, data, meta{backend, model, elapsed_ms, requests,
      cache_hits, input_tokens, cost_usd, threshold, request_id},
      error{kind, message, hint, example} | null }

Branch on `exit_code` (0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input,
7 child failed, 130 declined), then read `data`. Never parse human output.

## Verbs
- `pick "<intent>"` (stdin lines) → `data.matches[{line, text, p}]`; exit 3 = nothing fits.
- `why` (stdin: failing output, or `why -- <cmd...>` to run it and capture stdout+stderr) →
  `data.causes[{line, text, p, context[]}]`, `data.child_exit`; `data.hint` explains an exit 3
  on output with no error-like line (usually stderr was not piped).
- `run "<intent>"` → `data.tool, argv[], complete, blocked, executed`. Machine mode never executes
  unless `--exec --yes`; the child's stdout is redirected to stderr so stdout stays one envelope.
  `blocked` names a tool grevi refuses to run (rm, dd, mkfs*, sudo, wrappers such as
  sh/bash/env/xargs/find that would run another program, interpreters such as
  python*/perl*/ruby*/node*/php*/lua* that take program text, ...): run `argv` yourself under
  your own rules. Use `--dry-run` to route only. Only exact no-argument `true`, `false`, `pwd`,
  and `ls` forms are validated for execution; everything else has `complete=false` and a
  `blocked` reason, even without placeholders. Execution assumes trusted PATH contents.
- `is "<condition>"` (stdin) → `data.p, verdict`; exit 0 yes, 1 no, 3 unsure. Oversized input
  abstains before an API call, with `p:null`, `verdict:"unsure"`, `truncated:true` and a reason.
- `add "<topic>"` → `data.hunks[{file, header, p, staged}]`; stages only unstaged hunks of tracked files, index only, never commits; machine mode stages only with `--yes` (else exit 130); exit 3 = no hunk is about the topic. A hunk over 3,000 characters is an input error before API calls or staging.
- `sort <dir> [--into <root>]` → `data.moves[{from, to, p}], skipped[{file, reason}], undo_log, applied`; proposes a home among existing folders (depth ≤ 2). Dry-run by default. Apply/undo use atomic no-replace moves and a unique JSONL recovery log with absolute path bytes and file identity. Old TSV logs are rejected. Failures identify the log and completed progress. Symlink entries are skipped; concurrent replacement of source files is unsupported. Same volume only; exit 3 = nothing to move (or nothing restored).

## Rules for agents
- On a very long log, `why` keeps the lines around every error-like line within a 4,000-line
  budget. `data.considered` and `data.total` say how much it looked at. If `considered` is far
  below `total` and the answer looks like a symptom, cut the log to the failing step and ask again.
- Exit 4 with `HTTP 429` is the backend's rate limit: wait, or lower `GREVI_CONCURRENCY`.
- `p` is a backend score; application calibration requires evidence for the task and question type.
  TypeSafe Noul and classifier binary Choice are not assumed interchangeable. Raising `-t`
  changes the policy but does not validate incomplete evidence or unsafe actions.
- Exit 3 is an answer, not an error: nothing fits, or the evidence is ambiguous. Escalate or ask.
- Results always point into your input, the installed tools, or a tool's man page. Nothing is generated.
- Input is read as data, but the model is not hardened against instructions embedded in it: text
  under your control is fine; do not use `is` or `pick` as a security gate on untrusted text.
- TypeSafe defaults to `jev-1.13.0`; `--model jev-latest` follows its moving alias. Classifier
  selects its model server-side and rejects explicit `--model`/`GREVI_MODEL` overrides.
- No key is required: without one grevi asks classifier.dev, which runs the same Jev model and
  serves it free. `meta.backend` (`typesafe` or `classifier`) says which API answered, `meta.model`
  which build of Jev. `GREVI_BACKEND` forces one; `capabilities.backends` lists both with their
  limits. On `classifier` a question takes at most 100 options; the input takes 32,000 UTF-16
  code units and a request 20 questions. Serialized dimension definitions take at most 16,000
  UTF-16 code units. grevi splits dimensions and rejects oversized fields or option sets
  locally; backend translations can change answers and confidence.
- Cost is in `meta.cost_usd`, and is `0` on `classifier` because the service is free; repeated
  identical questions hit the local cache (`meta.cache_hits`), which never crosses backend,
  endpoint or decision-contract versions. `meta.requests` counts attempted inference POSTs,
  including retries and failures; it excludes prewarm and health GETs.
- Without the cache, the same request moves `p` by up to 0.06 between runs (measured on
  jev-1.13.0): a `p` within 0.06 of the threshold can flip. `is` has `--band` for that; the
  other verbs do not, so re-run with `--no-cache` before acting on such a value.
- `-n N` (`pick`, `why`) is ranked by a "which one" answer that is reliable at the top only:
  entries past the third are candidates, not a ranking.
- Errors carry `error.example`: a corrected command you can run next. `meta.request_id` is the
  TypeSafe request id of the last Jev request (`null` if none was made or every answer came from
  the cache; `health` does not record one): quote it when reporting an API problem.

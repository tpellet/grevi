# grevi — robot mode

Start here: `grevi capabilities --json`. Every command accepts `--json` (alias `--robot`) or
`--format json|jsonl|toon` and then prints exactly one envelope on stdout, usage errors included:

    { ok, command, version, exit_code, data, meta{model, elapsed_ms, requests, cache_hits,
      input_tokens, cost_usd, threshold, request_id}, error{kind, message, hint, example} | null }

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
  your own rules. Use `--dry-run` to route only. `complete=false` means `<VALUE>` placeholders remain.
- `is "<condition>"` (stdin) → `data.p, verdict`; exit 0 yes, 1 no, 3 unsure.
- `add "<topic>"` → `data.hunks[{file, header, p, staged}]`; stages only unstaged hunks of tracked files, index only, never commits; machine mode stages only with `--yes` (else exit 130); exit 3 = no hunk is about the topic.
- `sort <dir> [--into <root>]` → `data.moves[{from, to, p}], skipped[{file, reason}], undo_log, applied`; proposes a home among the existing folders under `root` (default `dir`, depth ≤ 2) for each file directly in `dir`. Dry-run by default: `--apply` moves and writes an undo log (`data.undo_log`), `--undo <log>` moves the files back. Never overwrites, never deletes, same volume only; exit 3 = nothing to move (or nothing restored).

## Rules for agents
- Treat `p` as calibrated: 0.8 ≈ right 80% of the time across many calls. Raise `-t` for costly actions.
- Exit 3 is an answer, not an error: nothing fits, or the evidence is ambiguous. Escalate or ask.
- Results always point into your input, the installed tools, or a tool's man page. Nothing is generated.
- Input is read as data, but the model is not hardened against instructions embedded in it: text
  under your control is fine; do not use `is` or `pick` as a security gate on untrusted text.
- The default model is pinned (`jev-1.13.0`); `--model jev-latest` follows TypeSafe's moving alias
  and may shift probabilities against the 0.5 threshold.
- Cost is in `meta.cost_usd`; repeated identical questions hit the local cache (`meta.cache_hits`).
- Without the cache, the same request moves `p` by up to 0.06 between runs (measured on
  jev-1.13.0): a `p` within 0.06 of the threshold can flip. `is` has `--band` for that; the
  other verbs do not, so re-run with `--no-cache` before acting on such a value.
- `-n N` (`pick`, `why`) is ranked by a "which one" answer that is reliable at the top only:
  entries past the third are candidates, not a ranking.
- Errors carry `error.example`: a corrected command you can run next. `meta.request_id` is the
  TypeSafe request id of the last Jev request (`null` if none was made or every answer came from
  the cache; `health` does not record one): quote it when reporting an API problem.

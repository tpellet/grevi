---
name: grevi
description: Use the grevi CLI to point at the right thing among real things - the line in a list that matches an intent, the line that broke a failing command, the installed tool for a task, the unstaged hunks about a topic, or a yes/no/unsure verdict on text. Use when choosing among existing items or root-causing build/test output. Do not use to generate code or text, or as a security gate on untrusted input.
---

# grevi

grevi points into your input, the installed tools or a man page; it never generates. Every answer
has a calibrated `p`, and "nothing fits" is a real answer.

## Check it is there

```sh
grevi health --json        # exit 0: key accepted, API reachable; exit 5: no or bad key
```

## Always machine mode

Pass `--json` and branch on `exit_code` (same as the process exit code), then read `data`:

- 0 ok · 1 no (`is`) · 3 abstain: nothing fits or unsure. It is an answer; do not retry, escalate or ask.
- 2 usage · 4 API unavailable · 5 auth · 6 input · 7 child failed · 130 declined.
- On error, `error.example` is a corrected command to try next.

## Verbs

```sh
git branch | grevi pick --json "payment timeout fix"   # data.matches[{line, text, p}]
cargo build 2>&1 | grevi why --json                     # data.causes[{line, text, p, context[]}]
grevi why --json -- cargo test                          # runs it, captures stdout+stderr
grevi is --json "asks for a refund" < mail.txt          # data.p, data.verdict
grevi run --json --dry-run "count the lines in notes.txt"  # data.tool, data.argv[], data.blocked
grevi add --json --dry-run "the auth fix"               # data.hunks[{file, header, p, staged}]
grevi sort --json ~/Downloads                           # dry run: data.moves[{from, to, p}]
```

Pipe `2>&1` into `why`: compilers write errors to stderr.

## Safety

- `run`: use `--dry-run`, then run `data.argv` yourself under your own rules. Never pass
  `--exec --yes` unless the user asked for grevi to execute. `data.blocked` names a tool grevi
  refuses to run; `complete=false` means a `<VALUE>` placeholder remains.
- `add`: `--dry-run` first; `--yes` stages (index only, never commits) only with the user's say-so.
- `sort`: dry run by default; `--apply` moves files and writes `data.undo_log` for `--undo`.
- Raise `-t` for costly actions. A `p` within 0.06 of the threshold can flip on a re-run.

## Source of truth

`grevi capabilities --json` (commands, flags, exit codes, limits) and `grevi robot-docs guide`
(the agent handbook). When this file and those differ, they win.

---
name: jevify
description: Use the jevify CLI when a question is about meaning and grep or keywords cannot ask it. Find the root cause in a long build, test or CI log, including when the line that explains it holds no word like "error". Triage many texts (tickets, mails, files, commits, search results) by a yes/no question without reading them. Pick one item from a list by description (a branch, a commit, a file, a process, a history line). Stage only the git changes that belong to one topic, which git add -p cannot do without a terminal. Find which installed tool does a task. Propose folders for files with meaningless names. Do not use to generate code or text, to count or judge quality, or as a security gate on untrusted input.
---

# jevify

jevify answers a question about text that already exists: your input, the installed tools, a man
page, the folders on disk. It selects and never generates, so an answer is always something you
can check. Scores depend on the backend and task; "nothing fits" and insufficient evidence
are real answers, not permission to invent a match.

## When it beats what you already have

You have `grep` and you can read files. Reach for jevify when those two run out:

- **A long log, or a grep that found the symptom.** `grep -iE "error|fail"` finds the line that
  says something failed. The line that says why often holds none of those words (an assertion's
  `left:`/`right:` values, "could not match actual sql"). `why` filters long logs into bounded
  evidence and returns a selected line with its number and context. Call it before you read a
  log of more than a few hundred lines, and whenever your grep named a failing test but not the
  reason.
- **Many texts, one question.** Do not read 200 tickets to find the 10 that matter. Loop `is`
  over them and read the exit codes. Oversized texts abstain without a model call; read or
  scope those separately. Then read the few that said yes or unsure.
- **One item out of many, described and not named.** "The commit where we changed the pricing",
  "the branch with the timeout fix", "the process that is draining the battery". `pick` matches
  by meaning, so the description and the line need no word in common. Use `-n 3` to see the
  runners-up.
- **Part of a working tree.** `git add -p` is interactive, so you cannot use it. `add` stages the
  hunks that belong to one topic and leaves the others unstaged.
- **A task with no tool you are sure of.** `run --dry-run --no-args` searches every command on
  the PATH by what its man page says it does. It finds tools you would not think of
  (`caffeinate` for "keep my mac awake", `networkQuality` for "test my connection speed") and
  tells you which of several candidates is installed. Read `data.tool` and
  `data.alternatives`, then check the flags in the man page yourself.
- **Files whose names say nothing.** `sort` reads the content and proposes one of the existing
  folders for each file.

Skip jevify when a literal search answers the question, when the input is short enough to read, or
when you already know the exact command.

## Ask literal questions

The model judges the statement you wrote, word for word. Write what must be true of the text.

- Good: `"the customer is about to stop being a customer"`. Weak: `"this customer is about to
  leave"`, which also says yes to an employee who is leaving their company.
- Describe the thing, not what you will do with it: `"the line with the failing assertion"`, not
  `"what should I fix"`.
- One question per call. For "A or B", make two calls.
- English works best. jevify does not count, do arithmetic, compare dates or judge quality.

## Check it is there

```sh
jevify health --json        # exit 0: API reachable (data.backend names it); exit 5: bad or missing key
```

## Always machine mode

Pass `--json` and branch on `exit_code` (same as the process exit code), then read `data`:

- 0 ok · 1 no (`is`) · 3 abstain: nothing fits or unsure. It is an answer; do not retry, escalate or ask.
- 2 usage · 4 API unavailable · 5 auth · 6 input · 7 child failed · 130 declined.
- On error, `error.example` is a corrected command to try next.

## Verbs

```sh
git branch | jevify pick --json "payment timeout fix"   # data.matches[{line, text, p}]
jevify pick --files . --json "where retries back off"    # a file by what it is about; text = path
cargo build 2>&1 | jevify why --json                     # data.causes[{line, text, p, context[]}]
jevify why --json -- cargo test                          # runs it, captures stdout+stderr
jevify is --json "asks for a refund" < mail.txt          # data.p, data.verdict
jevify run --json --dry-run "count the lines in notes.txt"  # data.tool, data.argv[], data.blocked
jevify add --json --dry-run "the auth fix"               # data.hunks[{file, header, p, staged}]
jevify sort --json ~/Downloads                           # dry run: data.moves[{from, to, p}]
```

Pipe `2>&1` into `why`: compilers write errors to stderr. On a very long log, `why` keeps the
lines around every error-like line, from the top first, within a 4,000-line budget;
`data.considered` and `data.total` tell you how much it looked at. If `considered` is far below
`total` and the answer looks like a symptom, cut the log to the failing job or step and ask again.

## Patterns

```sh
# Triage many texts and read none of them: one line per text comes back.
for f in tickets/*.txt; do
  jevify is "the customer is about to stop being a customer" < "$f" >/dev/null 2>&1
  echo "$f $?"                                   # 0 yes · 1 no · 3 unsure
done

# Root cause of a CI run, however long the log is.
gh run view --log-failed | jevify why --json

# Feed the choice to the next command.
git show "$(git log --oneline | jevify pick "the commit that renamed the project" | cut -d' ' -f1)"

# Stage one topic: look at the scores first. Stage only if the user asked you to stage.
jevify add --json --dry-run "the token expiry fix"
jevify add --json --yes "the token expiry fix"
```

Treat exit 3 in a loop as "a human or a closer read decides", not as no. Exit 4 with HTTP 429
means the free backend's rate limit: wait and continue, or lower `JEVIFY_CONCURRENCY`. Identical
requests are cached for 7 days, so a re-run of the same loop is free and fast.

Never pipe secrets. jevify masks obvious tokens before sending, but that is best effort, and
whatever you pipe (a shell history, a log with credentials) goes to the API.

## Safety

- `run`: use `--dry-run`, then run `data.argv` yourself under your own rules. Never pass
  `--exec --yes` unless the user asked for jevify to execute. `data.blocked` names a tool jevify
  refuses to run or whose grammar is unvalidated. Only exact no-argument `true`, `false`,
  `pwd`, and `ls` forms can be complete and execute, assuming trusted PATH contents.
  Other flags/operands/commands remain proposals even with `--exec --yes`.
- `add`: stages single hunks, not whole files, so it can split one file's changes. `--dry-run`
  first; `--yes` stages (index only, never commits) only with the user's say-so. Hunks above
  3,000 characters are rejected before classification or staging; evidence is never clipped.
- `sort`: dry run by default; `--apply` uses atomic no-replace moves and a JSONL recovery log.
  Symlink entries are skipped; concurrent source replacement is unsupported. On failure,
  retain the journal path and reported progress for recovery.
- `is`: oversized input returns exit 3, `p:null`, `truncated:true`, without an API call.
- A higher threshold does not make incomplete evidence or unvalidated actions safe. TypeSafe
  Noul and classifier binary Choice scores do not share established application calibration.

## Source of truth

`jevify capabilities --json` (commands, flags, exit codes, limits) and `jevify robot-docs guide`
(the agent handbook). When this file and those differ, they win.

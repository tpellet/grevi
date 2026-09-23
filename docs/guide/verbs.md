# Verbs

## Global flags

Every command accepts these before or after the verb:

| Flag | Meaning |
|:---|:---|
| `--json`, alias `--robot` | one JSON envelope on stdout |
| `--format human\|json\|jsonl\|toon` | output format, overriding `--json` |
| `-t, --threshold <0..1>` | backend fit threshold, default 0.5 |
| `--model <id>` | TypeSafe model, default `jev-1.13.0`; unsupported on classifier |
| `--no-cache` | bypass the answer cache |
| `--verbose` | probabilities, request count, cost and timing on stderr; no short flag |
| `-V, --version` | version |

`-v` belongs to `filter` and means inversion. The output side in one table:

| You want | Verb | Classical twin | Returns |
|:---|:---|:---|:---|
| the line that explains a failure | `why` | — | numbered lines with context |
| one record out of many | `pick 'description'` | `fzf --filter` | an input record |
| only the records that matter | `filter 'statement'` | `grep` | a subset of the input |
| a tag on each record, to sort or count them | `label a,b,c` | an `awk` key | each record with its label |
| a decision to branch on | `is 'statement'` | `test` | an exit code |

Text beginning with `-` goes after `--`. The threshold applies to yes/no fit scores, not to
relative selection ranks.

## fill

```sh
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
printf 'retry_backoff\nparse_header\n' | jevify fill --dry-run -- cargo test '@{-:the retry test}'
printf 'A crash with no reproduction steps.\n' | jevify fill --dry-run -- printf '%s\n' \
  '@{one:bug|feature|docs:what kind of report is this}' '@{flag:--draft:the report lacks steps to reproduce}'
```

`fill [--dry-run] [-q] [--candidates FILE] [--context FILE] [--field N | --key KEY]
[-0 | --para] -- COMMAND ARGS...` fills markers then becomes the caller-written command,
without a shell. All markers resolve together; if any abstains, nothing runs. `--dry-run`
prints shell-quoted argv and exits 0 on success; its last stderr line reads
`jevify fill: would run 'git' 'switch' 'ticket/TPE-791'`, while a run ends with
`jevify fill: exec ...` before the command takes over. Inspect it, never `eval` it. Machine output
requires `--dry-run` and returns `argv`, `reason`, and
`markers[{arg,kind,reason,handle,p,candidates,total,omitted}]`.

| Family | Marker argument | Source |
|:---|:---|:---|
| existing things | `'@{branch:the auth refactor}'`, `'@{commit:made folder moves atomic}'`, `'src/@{file:parses the marker}'`, `'@{pod:the payment worker}'` | a lister per kind: `branch`, `commit`, `file`, `dir`, `tool`, and the recipes `pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod` |
| supplied records | `'@{-:the retry test}'` | stdin or `--candidates FILE` |
| caller options | `'@{one:bug\|feature\|docs:what kind of report is this}'` | options in the marker, judged against context |
| caller options | `'@{flag:--draft:the report lacks steps to reproduce}'` | yes keeps, no removes, unsure abstains |

```sh
jevify fill --dry-run -- git revert '@{commit:made folder moves atomic}'
jevify fill --dry-run -- cat 'src/@{file:parses the marker}'
```

Each kind lists its own candidates: `branch` runs `git for-each-ref`, newest first, then
`git log` for richer finalist evidence; `commit` runs `git log`; `file` and `dir` run
`git ls-files`; a recipe kind runs the owning tool's listing. `capabilities.kinds` lists the
exact argv of every kind, user recipes included; `-`, `one` and `flag` run no lister. A
literal prefix ending in `/` narrows `file` and `dir`. [Kinds](kinds.md) has the table, the
recipe line and the rules. `--field N` extracts a 1-based whitespace field, `--key KEY` a JSON
handle, while the complete record is evidence. The default split is lines; `-0` reads NUL
records and `--para` paragraphs.

The whole marker argument is single-quoted, including any prefix or suffix, for example
`'--value=@{-:the retry test}'`. An apostrophe is `'\''`; marker escapes are `\}`, `\:` and
`\|`. Options of `one` are separated by `|` before the question's `:`. `flag` is a whole
argument. Unknown kinds, unclosed markers and no marker are usage errors, exit 2.
`@@{word:` spells literal `@{word:`. `'{user}@{host:>8}'` is an unknown kind;
`'{user}@@{host:>8}'` is literal. A marker does not survive a second shell (`ssh`, `make`, `xargs`).

stdin supplies either candidates for `-` or context for `one`/`flag`; never both. Supply the
other role with a file. If consumed, the command receives empty stdin; otherwise it inherits
stdin and the terminal. Environment and working directory pass through unchanged.

Each marker accepts 3,267 candidates keyless or 13,200 on TypeSafe, keeping three names per
window in the shortlist round; with one window, when the names leave a `branch`, `commit`,
`file` or `dir` undecided, every name not ruled out reaches the finals with its evidence, up to
24. Ordered kinds retain newest candidates and report `candidates N of M, newest first`;
unordered overflow is exit 6 `too_many`. `one` accepts 99 options keyless or 200 on TypeSafe;
overflow is exit 2. Every lister has one 20 s deadline; a tool that is missing, not logged in
or rate-limited is exit 6 `lister_failed` with the tool's own text.

Before execution, exits 2–6 mean nothing ran; after execution the command owns output, signals
and exit code, including 2–6. Status lines use the `jevify fill:` prefix; `-q` keeps only
`not run:`. Empty candidates produce this status without a model request:

```text
jevify fill: not run: arg 3 -: no_match; no record to choose from; candidates 0 of 0, omitted 0; model not requested
```

Exit 3 has `error: null`: `data.reason` is the first failed marker in argv order; every marker
has its own reason in `data.markers[]`. Reasons: `no_match`, `ambiguous`, `unsure_flag`,
`insufficient_evidence`. Read candidate coverage, inspect close handles, or write/drop an unsure
flag explicitly. Input error kinds (exit 6): `stdin_is_tty`, `lister_failed`, `too_many`,
`cannot_run`, `recipe_invalid`. Run a failed lister yourself; narrow oversized input.

Every answering model must be Jev, even under `--dry-run`. Otherwise exit 4 `api_unavailable`,
`answered by <model>, not Jev`; a missing name becomes `unknown` in `meta.model` and refuses too.
Allow dry runs freely; authorize execution per command prefix as the command itself is allowed.

## why

```sh
cargo build 2>&1 | jevify why
gh run view --log-failed | jevify why --json
```

`why [-C N] [-n N] [--no-save]` reads stdin and prints numbered cause lines with context.
`-C, --context` defaults to 3; `-n, --top` defaults to 1. `why` takes none of the split
options. It exits 0 when a cause is found and 3 when none fits. Compare `considered` with
`total` for evidence coverage.

Data: `causes[{line,text,p,context[]}]`, `any`, `considered`, `total`, `hint`, `saved_input`,
`complete`. The full raw input is saved unless `--no-save`; stderr says
`jevify why: full output: PATH`. A skipped or failed save reports the reason and sets
`complete=false`. A no-signal abstention carries a hint about piping stderr.

## pick

```sh
git branch | jevify pick 'the payment timeout fix'
git log --oneline | jevify pick -n 3 'when we changed the pricing'
git ls-files | jevify pick --files 'where man pages are parsed'
```

`pick '<intent>' [-n N] [--index | --files] [-0 | --para]` selects records, preserving their
bytes and input order. `-n, --top` defaults to 1; `--index` prints 1-based positions instead.
`-0` reads NUL-separated records; `--para` reads paragraphs; the two conflict.
Exit 0 found, 3 nothing fits. Input is not saved.

`--files` reads paths from stdin, ranks their names, then reads eligible excerpts of at most
24 finalists. Hidden and secret-looking paths remain candidates but receive no excerpt;
symlink files also receive no excerpt; a file that cannot be read is named on stderr with the
reason and competes on its name. Stderr reports `excerpts withheld: N` for both. There is no
directory enumeration by this flag and it conflicts with `--index`.

Data: `matches[{line,text,ordinal,p,lossy?}]`, `any`, `source` (`stdin` or `files`). Non-UTF-8
records have `lossy: true`; human output retains their exact bytes. Identical records are
ranked once and the first occurrence supplies the match. The limit is 20,000 distinct records,
with `too_many` (exit 6) above it. Selected evidence can be clipped; see [How it works](how-it-works.md).
Check the selection's exit code before passing its output as a command argument.

### pick --from

```sh
jevify pick --from branch 'the auth refactor'
jevify pick --from commit 'made folder moves atomic'
printf 'retry_backoff\nparse_header\n' | jevify pick 'the retry test'
```

`pick --from KIND '<intent>' [-n N]` prints handles of any kind of [Kinds](kinds.md) except
`one` and `flag`, without running a user command: a branch name, a commit OID, a path, a tool
name, or the handle of a recipe such as a PR number. stdin is plain `pick`; `--from -` is
exit 2. `--from` conflicts with `--files`, `--index`, `-0` and `--para`. Selection uses the
same ratio and fit gates as `fill`, with exit 3 for `no_match` or `ambiguous`. Stderr prints
`jevify pick: candidates N, windows W` before the first request, `of M, newest first` when an
ordered listing was cut, and `excerpts withheld: N` for `file` finalists.
Data: `matches[{text,ordinal,p,lossy}]`, `reason`, `any`, `source`, `candidates`, `total`,
`omitted`, `windows`, `finalists_per_window`.

Both forms of `pick` accept 9,801 candidates keyless or 20,000 on TypeSafe, with three,
two or one finalists per window as needed to fit the final comparison. Ordered kinds retain
the newest candidates and report coverage; unordered overflow is exit 6 `too_many`.

## filter

```sh
printf 'error: timeout\nbuild started\n' | jevify filter 'reports a network failure'
fd -0 -e txt | jevify filter -0 --files 'asks for a refund'
```

`filter '<statement>' [-v] [-c] [--strict] [-0 | --para] [--files] [--no-save]` judges each
distinct record and prints matching records, retaining repeated occurrences and input order.

| Flag | Meaning |
|:---|:---|
| `-v` | invert yes/no selection |
| `-c` | print the kept count instead of records in human mode |
| `--strict` | drop unsure records (otherwise retained, even with `-v`) |
| `-0` | NUL-separated records |
| `--para` | paragraphs; conflicts with `-0` |
| `--files` | stdin paths with eligible file excerpts as evidence |
| `--no-save` | skip saving raw input |

Exit 0 kept some, 1 kept none, 3 every record unsure. The verdict is one-sided: a record is a
yes when P(holds) reaches the threshold plus a fixed band of 0.15, a no when P(does not hold)
reaches that same mark, and unsure otherwise; with the default threshold of 0.5 the mark is
0.65 on either side, and nothing is compared to the threshold minus the band. Hidden or
secret-looking paths and symlink files receive no excerpt.
Up to 60 records per classifier request are judged independently; TypeSafe batches 20 records
in shared state. The ceiling is 20,000 distinct records (`too_many`, exit 6).

Data: `records[{text,ordinal,p,verdict,lossy?,unreadable?}]`, `kept`, `total`, `unsure`,
`complete`, `saved_input`, `excerpts_withheld`. `records` contains the kept subset, including
under `-c`. Under `--files`, a file that cannot be read (missing, a directory, a permission or
sandbox denial) is never judged by its name: it is unsure with p 0, kept unless `--strict`, named
on stderr as `excerpt unreadable: PATH: REASON` and carries `unreadable`.
Stderr reports `jevify filter: kept N of M, U unsure, full output: PATH`, with withholding and
non-Jev model details when relevant. A skipped or failed save sets `complete=false`.
Human output can be a prefix if a later batch fails; an error exits nonzero and reports progress.

## label

```sh
gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c
printf 'crash on empty input\nadd a dark theme\n' | jevify label bug,feature,question
```

`label a,b,c [-0 | --para] [--files]` gives each distinct record one of the labels and prints
`LABEL<TAB>RECORD` for every record, in input order, the record unchanged after the tab. An
unsure record gets `?`: none of the labels fits it, or two fit it about as well. Labels are
comma-separated: at least two, distinct, none empty, none `?` or `NONE`. At most 99 labels on
classifier.dev and 200 on TypeSafe; more is exit 2 with the count.

| Flag | Meaning |
|:---|:---|
| `-0` | NUL-separated records |
| `--para` | paragraphs; conflicts with `-0` |
| `--files` | stdin paths with eligible file excerpts as evidence |

Exit 0 labelled, 3 every record unsure. The threshold plays no part: a label wins when it
beats the other labels and "none of them" clearly. The way back: for line records `cut -f2-`
gives the input back without its blank lines; with `-0` and `--para` the record follows the
tab unchanged. Nothing is saved. The record limits of `filter` apply: 60 records per
request on classifier.dev, 20 on TypeSafe, 20,000 distinct records (`too_many`, exit 6).

Data: `records[{label,text,ordinal,p,lossy?,unreadable?}]`, `labelled`, `total`, `unsure`,
`complete`, `excerpts_withheld`. `p` is the winning label's probability, or the best label's
under `?`. Under `--files`, a file that cannot be read is never labelled by its name: it is `?`
with p 0 without a request, named on stderr as `excerpt unreadable: PATH: REASON`, and its
record carries `unreadable`; it counts in `excerpts_withheld`.
Stderr reports `jevify label: N records, D distinct, R requests` before the first request and
`jevify label: labelled N of M, U unsure` at the end, with withholding and non-Jev model
details when relevant. Human output can be a prefix if a later batch fails.

## is

```sh
printf 'All tests passed.\n' | jevify is 'the tests passed' && printf 'ready\n'
printf 'Please refund order 42.\n' | jevify is 'asks for a refund' 'mentions an order'
jevify is 'asks for a refund' --context mail.txt
```

`is '<statement>' ['<statement>' ...] [--context FILE] [--band 0.15]` judges one context from
stdin or a file. Write the condition so that yes means act. One statement prints nothing;
several print `VERDICT<TAB>STATEMENT` lines with `yes`, `no` or `unsure`.
Exit 0 all yes, 1 any no, 3 otherwise. The unsure band accepts 0 through 0.5.

One-statement data: `p`, `verdict`, `truncated`. Several: `statements[{statement,verdict,p}]`,
aggregate `verdict`, `truncated`. Oversized context adds `reason`, reports null probabilities,
and abstains before inference. Evidence limits are about 96,000 characters on TypeSafe and
30,000 on classifier, within the 64 MiB input cap. Input is not saved.

## route

```sh
jevify route 'keep my mac awake for an hour'
```

`route <intent...>` searches installed commands by summaries and man-page evidence. It prints
a tool, summary and synopsis, with fit on stderr. No user command starts and no arguments are
selected. Exit 0 found, 3 nothing fits or a near tie. Data: `tool`, `summary`, `synopsis`, `fit`,
`ties[{tool,fit}]`, `alternatives[{tool,fit}]`; synopsis can be null without a man page.

Each finalist gets an absolute fit of its own, so several commands that all serve a task all
score high. When the runner-up is above the threshold and within 0.10 of the best, the two are
too close to tell apart: `route` exits 3, prints nothing on stdout, sets `tool` to null and
names them in `ties` (the best first) and on stderr (`too close to tell apart: host (0.96),
dig (0.95)`). The margin sits above the 0.06 jitter between identical uncached requests, so
one jitter width cannot turn a tie into a decision. The caller reads the names and writes the
command.

## add

```sh
jevify add --dry-run 'the token expiry fix'
```

`add '<topic>' [--dry-run | --yes]` scores complete unstaged hunks of tracked files. `--dry-run`
stages nothing; `-y, --yes` stages qualifying hunks without asking. Machine mode requires
`--yes` to stage, otherwise exit 130. Index only, never commits, no untracked or binary changes.

Exit 0 staged or scored, 3 no matching hunk, 6 empty or oversized input, 130 declined.
Data: `hunks[{file,header,p,staged}]`. Hunks over 3,000 characters and batches above the backend
evidence budget are rejected before inference or staging. No unseen suffix is staged.

## sort

```sh
jevify sort ~/Downloads
```

`sort <DIR> [--into ROOT] [--apply | --undo LOG]` proposes existing destination folders for
regular non-hidden files directly in DIR. `--into` supplies the destination root; eligible
folders are at depth at most two. File text is excerpted, not read in full.

`--apply` moves and writes a unique JSONL recovery journal; `--undo` restores matching files
to free original paths using that journal. Both use atomic no-replace operations. Symlink
entries are skipped; same-volume support is required. Concurrent source replacement is
unsupported. Failures name the recovery journal and completed progress. No confirmation prompt.

Exit 0 proposed, applied or restored, 3 nothing can be placed or restored, 6 input error.
Data: `moves[{from,to,p}]`, `skipped[{file,reason}]`, `undo_log`, `applied`.

### Filesystem failure matrix

`--apply` runs one journal per invocation: for each file, an intent line (paths and the
file's device and inode) is written and synced, the rename runs with no-replace semantics
(`renameat2(RENAME_NOREPLACE)` on Linux, `renameatx_np(RENAME_EXCL)` on macOS), then a
completion line is written and synced. Directory handles are opened component by component
without following links and anchor the rename; the source is rechecked by identity through
that handle. `--undo` reads the whole journal first and then, newest intent first, moves a
file back only when its original path is free and the destination still holds that identity.
`add` stages through `git apply --cached`, which writes `index.lock` and renames it over
`index`, so the index either changes in full or not at all.

Each cell says what happens on macOS (APFS, HFS+) and Linux (ext4, xfs, btrfs, tmpfs) and how
it is known: **refuses** stops with exit 6, names the journal and the count of confirmed
moves, and changes nothing else; **recovers** means `--undo` reconciles the file from the
journal; **unsupported** means the outcome is not guaranteed and is documented as such;
**untested** names the reason.

| Condition | move (`--apply`) | journal write | `--undo` | `add` index write |
|:---|:---|:---|:---|:---|
| source replaced during the move | macOS, Linux: **unsupported**. The identity check runs on the handle before the rename; a swap in the window after it moves the replacement. Tested for a swap before the check (refuses). | — | same as move: the identity check precedes the rename | — |
| destination appears during the move | macOS, Linux: **refuses** (`EEXIST` from the no-replace rename), the source stays; tested with a regular file and a dangling symlink appearing after the check | macOS, Linux: **refuses**; the journal is created with `O_EXCL` and a new name is tried up to 1,000 times | macOS, Linux: **refuses** that file (skipped, "original path occupied"), restores the others | macOS, Linux: **refuses**; a second `index.lock` makes `git apply` fail and nothing is staged |
| symlinked parent | macOS, Linux: **refuses** (`ELOOP` from `O_NOFOLLOW` on the component); tested by replacing a source parent with a link after the move: undo reports the failure and the file stays where it is | macOS, Linux: **refuses** when the cache directory is reached through a link that was replaced; the journal directory is canonicalized once at creation | as move | not applicable: git resolves its own paths |
| crash after the intent line, before the move | macOS, Linux: **recovers**; the file is still at its source, `--undo` skips it with "intent not performed" and exits 3 when nothing else moved; tested | — | — | — |
| crash after the move, before the completion line | macOS, Linux: **recovers**; the durable intent and the identity at the destination are enough, `--undo` moves the file back; tested with a missing and a torn completion line | — | — | git: **recovers**; a leftover `index.lock` makes the next `git apply` refuse until it is removed, and the index is unchanged |
| crash before the intent line is durable | macOS, Linux: **refuses** on `--undo` when the line is torn (exit 6, nothing moves because the rename only runs after the sync); an empty journal restores nothing (exit 3); tested | — | — | — |
| disk full at the journal creation | macOS, Linux: **refuses** before any move ("create recovery journal before moving") | same | — | — |
| disk full at the intent line | macOS, Linux: **refuses**, "confirmed completed N move(s)", the file stays; Linux: tested with `/dev/full` (`ENOSPC`); macOS: measured on a 2 MiB APFS image (the volume still creates the empty journal, the first step after it fails with `ENOSPC`, the file stays at its source and `--undo` reports it as not performed) | same | — | — |
| disk full at the move | macOS, Linux: **refuses**; a rename that fails leaves both names as they were (POSIX), the intent is durable and `--undo` reports the file as not performed. The rename's own `ENOSPC` is **untested** in the default gate: it needs a full volume; `JEVIFY_SMALL_VOLUME_DIR` opts a test into filling a dedicated small volume and checks that the file is at exactly one of its two paths | — | as move | git: **refuses**; `index.lock` cannot be written in full and `git apply` fails, nothing is staged; **untested** here |
| disk full at the completion line | macOS, Linux: **recovers**; the error names N + 1 confirmed moves and `--undo` reads the durable intent; tested with a torn completion line, `ENOSPC` itself covered by the opt-in test | same | — | — |
| unsupported filesystem (no atomic no-replace rename) | macOS, Linux: **refuses** (`EINVAL` or `ENOTSUP` from the rename flag), nothing falls back to a replacing rename; **untested**: no such filesystem in the gate (NFS and FUSE mounts are the usual cases) | macOS, Linux: **unsupported**; a journal on a filesystem without durable `fsync` may lose lines after a power loss | as move | git: **unsupported** on filesystems without atomic rename (git's own limit) |
| cross-volume destination | macOS, Linux: **refuses** ("--into must be on the same volume"); no copy is ever made | — | as move | — |

`--undo` after a partial failure is safe to run twice: the second run finds nothing to
restore and exits 3.

## Utility commands

| Command | Output / data | Exit |
|:---|:---|:---|
| `jevify capabilities --json` | command, flag, exit, environment, limit and safety contract | 0 |
| `jevify robot-docs` | handbook; `topic`, `text` | 0 |
| `jevify robot-docs commands` | command table; also accepts `guide`, `exit-codes`, `examples`, `privacy` | 0; 2 unknown topic |
| `jevify health --json` | `backend`, `base_url`, `key`, `api`, `latency_ms`, `models` | 0, 4, 5 |
| `jevify init agents` | bounded instruction block; `script` | 0 |
| `jevify init zsh` | shell snippet; `script`; also accepts `bash` | 0 |

Shell snippets define the comma alias and opt-in command-not-found hook for `route`.
No profile is edited. `health` checks reachability without an inference call.

## Common exit codes

0 success, 1 no, 2 usage, 3 abstain (`filter` and `label`: every record unsure), 4 unavailable
or quota exhausted, 5 TypeSafe auth, 6 input,
7 reserved, 130 declined at `add` confirmation. Common errors supplement the per-verb codes.
A `rate_limit_day` HTTP 429 exits 4 with `daily quota of the free backend reached`, without retry.
All machine formats carry the same envelope; see [Agents](agents.md).

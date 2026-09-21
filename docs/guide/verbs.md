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

`-v` belongs to `filter` and means inversion. Text beginning with `-` goes after `--`.
The threshold applies to yes/no fit scores, not relative selection ranks.

## why

```sh
cargo build 2>&1 | jevify why
gh run view --log-failed | jevify why --json
```

`why [-C N] [-n N] [--no-save]` reads stdin and prints numbered cause lines with context.
`-C, --context` defaults to 3; `-n, --top` defaults to 1. No split options are accepted.
Exit 0 found, 3 no cause fits. Compare `considered` with `total` for evidence coverage.

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
symlink files also receive no excerpt. Stderr reports `excerpts withheld: N`. There is no
directory enumeration by this flag and it conflicts with `--index`.

Data: `matches[{line,text,ordinal,p,lossy?}]`, `any`, `source` (`stdin` or `files`). Non-UTF-8
records have `lossy: true`; human output retains their exact bytes. Identical records are
ranked once and the first occurrence supplies the match. The limit is 20,000 distinct records,
with `too_many` (exit 6) above it. Selected evidence can be clipped; see [How it works](how-it-works.md).
Check the selection's exit code before passing its output as a command argument.

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

Exit 0 kept some, 1 kept none, 3 every record unsure. A fixed band of 0.15 around the threshold
defines unsure. Hidden or secret-looking paths and symlink files receive no excerpt.
Up to 1,000 records per classifier request are judged independently; TypeSafe batches 20 records
in shared state. The ceiling is 20,000 distinct records (`too_many`, exit 6).

Data: `records[{text,ordinal,p,verdict,lossy?}]`, `kept`, `total`, `unsure`, `complete`,
`saved_input`, `excerpts_withheld`. `records` contains the kept subset, including under `-c`.
Stderr reports `jevify filter: kept N of M, U unsure, full output: PATH`, with withholding and
non-Jev model details when relevant. A skipped or failed save sets `complete=false`.
Human output can be a prefix if a later batch fails; an error exits nonzero and reports progress.

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
selected. Exit 0 found, 3 nothing fits. Data: `tool`, `summary`, `synopsis`, `fit`,
`alternatives[{tool,fit}]`; synopsis can be null without a man page.

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

0 success, 1 no, 2 usage, 3 abstain, 4 unavailable or quota exhausted, 5 TypeSafe auth, 6 input,
7 reserved, 130 declined at `add` confirmation. Common errors supplement the per-verb codes.
A `rate_limit_day` HTTP 429 exits 4 with `daily quota of the free backend reached`, without retry.
All machine formats carry the same envelope; see [Agents](agents.md).

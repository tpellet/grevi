# Verbs

Every fact on this page comes from `grevi --help`, `grevi <verb> --help` and `grevi capabilities --json` on a release build of `main`, plus the source for the limits.

## Global flags

Every command accepts these, before or after the verb:

| Flag | Meaning |
|:---|:---|
| `--json` | Machine output: one JSON envelope on stdout (alias `--robot`) |
| `--format human\|json\|jsonl\|toon` | Output format; overrides `--json` |
| `-t, --threshold <0..1>` | Decision threshold on the calibrated probability (default 0.5; env `GREVI_THRESHOLD`) |
| `--model <id>` | TypeSafe model or alias (default `jev-1.13.0`; `jev-latest` moves with each release; env `GREVI_MODEL`) |
| `--no-cache` | Skip the local answer cache (env `GREVI_NO_CACHE`) |
| `-v, --verbose` | Print probabilities, request count, tokens, cost and timing on stderr |
| `-V, --version` | Print the version |

The threshold gates absolute yes/no answers only (`is`, `why`'s "does this log hold a failure", `run`'s "is this command a correct, direct way"). "Which one" answers are a choice against NONE and the threshold never touches them; [How it works](how-it-works.md) explains the split.

## Exit codes

| Code | Name | Meaning |
|---:|:---|:---|
| 0 | ok | success: yes, found, executed |
| 1 | no | `is`: the condition does not hold |
| 2 | usage | bad flag or missing argument |
| 3 | abstain | nothing fits, or unsure |
| 4 | unavailable | TypeSafe API unavailable after retries |
| 5 | auth | API key missing or rejected |
| 6 | input | input error: empty, too large, or unreadable |
| 7 | child_failed | `run`: the executed command failed |
| 130 | interrupted | interrupted, or declined at the confirmation |

2, 4, 5 and 6 can come from any command. The per-verb lists below give the codes a verb returns on its own.

## pick

Print the stdin line(s) that match an intent.

```
<stdin> | grevi pick "<intent>" [-n N] [--index]
```

| Flag | Meaning |
|:---|:---|
| `-n, --top <N>` | Print up to N matches, each ranked above "nothing fits" (default 1) |
| `--index` | Print 1-based line numbers instead of lines |

Exit 0 with the line(s) on stdout; 3 when nothing beats NONE. `data`: `matches[{line, text, p}]`, `any`.

```sh
ls ~/Downloads | grevi pick "last month's electricity bill"
git switch $(git branch | grevi pick "payment timeout fix")
ps -eo pid,comm,%cpu | grevi pick -n 3 "eating my battery"
```

Limits: 20,000 stdin lines (exit 6 past that; filter with `rg` or `head` first). Each line is clipped to 200–2,000 characters before it is sent. Blank lines and repeated lines are sent once; the first occurrence keeps its line number. Past 255 distinct lines the tournament runs (windows of 200); `-n` past 3 lists candidates without a ranking claim.

## why

Point at the root-cause line in failing output.

```
<cmd> 2>&1 | grevi why [-C N] [-n N]
grevi why [-C N] [-n N] -- <cmd...>
```

| Flag | Meaning |
|:---|:---|
| `-C, --context <N>` | Lines of context around the root cause (default 3) |
| `-n, --top <N>` | Report up to N causes, each ranked above "no failure" (default 1) |
| `-- <cmd...>` | Run this command via argv (never a shell), read its stdout and stderr interleaved as on a terminal, and point into that instead of stdin |

Exit 0 with the cause and its context on stdout; 3 when no line looks like a failure (with `data.hint` about stderr when stdin held no error-like line). `data`: `causes[{line, text, p, context[]}]`, `any`, `considered`, `total`, `hint`, `child_exit`.

```sh
cargo build 2>&1 | grevi why
grevi why -- cargo build
npm test 2>&1 | grevi why -C 5 -n 3
```

Compilers and test runners write errors to stderr: pipe `2>&1`, or use `--`. With `--`, a daemon the command leaves behind (a build server, a watcher) can keep the pipe open; grevi waits one second after the command exits, then uses what it captured.

Limits: at most 4,000 lines are sent after a local prefilter (blank lines and repeats dropped; past 1,500 distinct lines, neighbourhoods of error-like lines from the top plus the tail of the log). Every line is clipped like `pick`'s.

## is

Exit 0 if stdin satisfies the condition, 1 if not, 3 if unsure. Prints nothing in human mode.

```
<stdin> | grevi is "<condition>" [--band 0.15]
```

| Flag | Meaning |
|:---|:---|
| `--band <0..=0.5>` | Unsure band around the threshold (default 0.15) |

With the defaults: yes at `p` ≥ 0.65, no below 0.35, unsure in between. `-t` moves the centre, `--band` the width; `--band 0` removes the unsure verdict. `data`: `p`, `verdict`, `truncated`.

```sh
grevi is "asks for a refund" < mail.txt && ./refund
grevi is "asks for a refund" < mail.txt; case $? in 0) ./refund;; 1) ./archive;; 3) ./ask;; esac
grevi is --band 0.05 "is a stack trace" < crash.log
```

Limits: about 96,000 characters of stdin are sent (head and tail when the input is longer; `data.truncated` says so). The model reads input as data but is not hardened against instructions inside it: do not use `is` as a security gate on text you do not control. `is` answers what a text is, not how many, how good or when: counting, arithmetic and dates are unreliable conditions.

## run

Route an intent to an installed tool, point at flags from its man page, confirm, run.

```
grevi run [--dry-run | --yes | --exec --yes] [--no-args] <intent...>
```

The request is the positional arguments joined with spaces; flags may follow it (`grevi run burn a dvd --dry-run`).

| Flag | Meaning |
|:---|:---|
| `--dry-run` | Only route and propose; never execute |
| `-y, --yes` | Run without asking |
| `--exec` | Allow execution in machine mode (requires `--yes`) |
| `--no-args` | Route only; do not point at flags or files |

Exit 0 when the command ran and exited 0 (or, with `--dry-run`, when a tool was routed); 3 when no tool fits; 7 when the executed command failed; 130 when you declined at the prompt. `data`: `tool`, `fit`, `argv[]`, `flags[]`, `complete`, `blocked`, `executed`, `child_exit`, `alternatives[]`.

```sh
grevi run "burn a dvd from this iso"
grevi run --dry-run "count the lines in notes.txt"
grevi run --no-args "what's using port 8080"
grevi run --json --dry-run "extract foo.tar.gz"
```

How a run goes: grevi reads the inventory of tools on your PATH with their one-line man-page summaries, asks Jev which tool answers the request (route), asks whether the proposed command is a correct, direct way to do it (fit, the answer `-t` gates), then points at flags in that tool's man page and at file names in the current directory that contain a word of the request (arguments). Three rounds of requests; `--no-args` stops after two. In human mode it prints the command and asks; on a terminal without a TTY and without `--yes` it prints and stops.

Safety, always on:

- Commands run via argv, never through a shell. Flags come from man pages; no binary is ever probed with `--help`.
- Some tools are never executed, whatever the confidence: the destructive set (`rm`, `rmdir`, `dd`, `mkfs*`, `newfs*`, `fdisk`, `diskutil`, `shred`, `srm`, `wipefs`, `sudo`, `su`, `doas`, `kill`, `killall`, `pkill`, `reboot`, `halt`, `shutdown`, `poweroff`, `init`, `telinit`, `launchctl`, `systemctl`), the wrappers that would run another program named in their arguments (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `env`, `xargs`, `nohup`, `nice`, `timeout`, `time`, `exec`, `eval`, `command`, `find`, `watch`, `parallel`, `osascript`) and the interpreters that take program text as a flag value (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`, versioned names included). grevi shows the command, sets `data.blocked`, and leaves it to you. The list is by tool name only: `chmod -R` is not on it.
- In machine mode (`--json`) nothing executes unless `--exec --yes` is given, and the child's stdout goes to stderr so stdout stays one envelope.

`complete=false` in `data` means the argv still holds a `<VALUE>` placeholder the man page could not fill. Argument pointing is measured at 5 of 20 for "all required flags present" ([README, Numbers](../../README.md#numbers)), so read `argv` as a proposal and prefer `--no-args` when you can write the flags yourself.

## add

New in 0.2.0. Stage only the git hunks about a topic.

```
grevi add [--dry-run | --yes] "<topic>"
```

| Flag | Meaning |
|:---|:---|
| `--dry-run` | Score the hunks; stage nothing |
| `-y, --yes` | Stage without asking |

Exit 0 when hunks were staged (or scored, with `--dry-run`); 3 when no hunk is about the topic; 6 when there are no unstaged changes to tracked files; 130 when you declined. `data`: `hunks[{file, header, p, staged}]`.

```sh
grevi add --dry-run "the auth fix"
grevi add --yes "the auth fix" && git commit
```

Each unstaged hunk of a tracked file is scored against the topic as an absolute yes/no; hunks at or above the threshold are staged. Works from any subdirectory of the repo. It touches the index only: never commits, never stages untracked files, never stages binary changes. In machine mode it stages only with `--yes`; otherwise exit 130 and nothing is staged. Each hunk (header plus body) is clipped to 3,000 characters before it is sent.

## sort

New in 0.2.0. Propose moving files into existing folders by meaning.

```
grevi sort <dir> [--into <root>] [--apply | --undo <log>]
```

| Flag | Meaning |
|:---|:---|
| `--into <root>` | Root whose sub-folders (depth ≤ 2) are the destinations (default: `<dir>`) |
| `--apply` | Move the files (dry run otherwise) and write an undo log |
| `--undo <log>` | Move files back using a log written by `--apply` |

Exit 0 with one proposed (or applied) move per file; 3 when nothing can be placed (or, with `--undo`, nothing restored); 6 when there are no folders under the root to sort into. `data`: `moves[{from, to, p}]`, `skipped[{file, reason}]`, `undo_log`, `applied`.

```sh
grevi sort ~/Downloads                      # dry run: proposes a folder per file
grevi sort ~/Downloads --apply              # moves, writes an undo log
grevi sort ~/Downloads --undo <log>         # moves them back
grevi sort ~/Desktop --into ~/Documents     # files from one place, folders from another
```

`sort` looks at each file directly in `<dir>` (not recursive, hidden files skipped) and asks Jev which of the existing folders under the root, up to two levels deep, is its home; a file the model cannot place, or places below the threshold, stays where it is and appears in `skipped`. Dry run by default. `--apply` renames the files and appends to an undo log (`sort-undo-<timestamp>.tsv` in the cache directory) after every move, so an interrupted run is still undoable. `--undo <log>` restores every file whose original path is still free.

It never overwrites a file, never deletes one, and moves within one volume only (`--into` must be on the same volume as `<dir>`). What is sent: the file names, the first 2,000 characters of each text file (or of a PDF's first two pages, when `pdftotext` is installed) and the folder names under the root, all after secret masking.

## Utility commands

### health

```sh
grevi health
```

Checks the key and TypeSafe reachability. Exit 0 ok, 4 unavailable, 5 auth. Makes no Jev request, so `meta.request_id` stays `null`.

### init

```sh
eval "$(grevi init zsh)"      # or bash
```

Prints the shell snippet: the `,` alias for `grevi run` (with `noglob` in zsh) and an opt-in command-not-found hook enabled by `GREVI_CNF=1` that routes unknown commands of three or more words to `grevi run`. See [Getting started](getting-started.md#the--alias).

### capabilities

```sh
grevi capabilities --json
```

Describes commands, flags, exit codes, environment variables, limits, the envelope, four workflows and the safety rules, as data. The source of truth for [Configuration](configuration.md).

### robot-docs

```sh
grevi robot-docs [guide|commands|exit-codes|examples|privacy]
```

Prints the agent handbook, [docs/ROBOT_MODE.md](../ROBOT_MODE.md), whole or one topic.

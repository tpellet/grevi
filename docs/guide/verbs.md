# Verbs

Every fact on this page comes from `grevi --help`, `grevi <verb> --help` and `grevi capabilities --json` on a release build of `main`, plus the source for the limits.

## Global flags

Every command accepts these, before or after the verb:

| Flag | Meaning |
|:---|:---|
| `--json` | Machine output: one JSON envelope on stdout (alias `--robot`) |
| `--format human\|json\|jsonl\|toon` | Output format; overrides `--json` |
| `-t, --threshold <0..1>` | Decision threshold on the backend score (default 0.5; env `GREVI_THRESHOLD`) |
| `--model <id>` | TypeSafe model or alias (default `jev-1.13.0`; `jev-latest` moves with each release; env `GREVI_MODEL`) |
| `--no-cache` | Skip the local answer cache (env `GREVI_NO_CACHE`) |
| `-v, --verbose` | Print probabilities, request count, tokens, cost and timing on stderr |
| `-V, --version` | Print the version |

The threshold gates absolute yes/no answers only (`is`, `why`'s "does this log hold a failure", `run`'s "is this command a correct, direct way"). "Which one" answers are a choice against NONE, and the threshold never touches them. [How it works](how-it-works.md) explains the split.

## Exit codes

| Code | Name | Meaning |
|---:|:---|:---|
| 0 | ok | success: yes, found, executed |
| 1 | no | `is`: the condition does not hold |
| 2 | usage | bad flag or missing argument |
| 3 | abstain | nothing fits, or unsure |
| 4 | unavailable | the API is unavailable after retries |
| 5 | auth | API key missing or rejected (the `typesafe` backend only) |
| 6 | input | input error: empty, too large, or unreadable |
| 7 | child_failed | `run`: the executed command failed |
| 130 | interrupted | interrupted, or declined at the confirmation |

2, 4, 5 and 6 can come from any command. The per-verb lists below give the codes a verb returns on its own.

## pick

Find one item in a list by describing it. Pipe a list into `pick` and describe the item you want. It prints the line that fits your description, even when the two share no word.

```
<stdin> | grevi pick "<intent>" [-n N] [--index]
grevi pick --files <DIR> "<intent>" [-n N]
```

| Flag | Meaning |
|:---|:---|
| `-n, --top <N>` | Print up to N matches, each ranked above "nothing fits" (default 1) |
| `--index` | Print 1-based line numbers instead of lines |
| `--files <DIR>` | Choose among the files under DIR instead of stdin lines; prints the path |

Exit 0 with the line(s) on stdout. Exit 3 when no line fits better than "nothing fits" (NONE). `data`: `matches[{line, text, p}]`, `any`, `source` (`stdin` or `files`).

`--files` finds a file by what it is about. The candidates are the regular files under DIR. Inside a git work tree the list comes from git, so `.gitignore` applies. Hidden files and directories, symlinks and names that are not UTF-8 are skipped. The first round ranks the path names only. The second round reads the first 2,000 characters of at most 24 finalist files, so a file whose name says nothing can still win on its content, provided its name reached the finals. The match prints as a path that works from the current directory; a name that starts with `-` prints as `./-name`. `--index` does not apply. More than 20,000 files is exit 6: choose a narrower DIR. An empty DIR is exit 6 with no request.

```sh
code "$(grevi pick --files . "where man pages are parsed")"
grevi pick --files ~/Downloads -n 3 "the tax form"
```

Quote the substitution and check the exit code in scripts: when nothing fits, `pick` prints nothing, and the outer command still runs with an empty argument.

```sh
history | grevi pick "how I made that gif from a screen recording"
git switch $(git branch | grevi pick "payment timeout fix")
ps -eo pid,comm,%cpu | grevi pick -n 3 "eating my battery"
```

The first command prints a line such as `ffmpeg -i screen.mov -vf "fps=12,scale=900:-1" -loop 0 demo.gif`. In zsh, write `history 1` to get the whole history. Whatever you pipe goes to the API, your shell history included ([PRIVACY.md](../../PRIVACY.md)).

Limits: `pick` takes 20,000 stdin lines and exits 6 past that, so filter with `rg` or `head` first. Each line is clipped to 200–2,000 characters before it is sent. Blank lines and repeated lines are sent once, and the first occurrence keeps its line number. Past 255 distinct lines the tournament runs (windows of 200). `-n` past 3 lists candidates without a ranking claim.

## why

Find the error in the output of a failed command. Pipe the output of a build, a test run or a CI job into `why`. It prints the line that caused the failure, with its line number and a few lines around it.

```
<cmd> 2>&1 | grevi why [-C N] [-n N]
grevi why [-C N] [-n N] -- <cmd...>
```

| Flag | Meaning |
|:---|:---|
| `-C, --context <N>` | Lines of context around the root cause (default 3) |
| `-n, --top <N>` | Report up to N causes, each ranked above "no failure" (default 1) |
| `-- <cmd...>` | Run this command via argv (never a shell), read its stdout and stderr interleaved as on a terminal, and point into that instead of stdin |

Exit 0 with the cause and its context on stdout. Exit 3 when no line looks like a failure; `data.hint` then mentions stderr if stdin held no error-like line. `data`: `causes[{line, text, p, context[]}]`, `any`, `considered`, `total`, `hint`, `child_exit`.

```sh
cargo build 2>&1 | grevi why
grevi why -- cargo build
npm test 2>&1 | grevi why -C 5 -n 3
gh run view --log-failed | grevi why
```

Compilers and test runners write errors to stderr, so pipe `2>&1` or use `--`. With `--`, a daemon the command leaves behind (a build server, a watcher) can keep the pipe open. grevi waits one second after the command exits, then uses what it captured.

Limits: at most 4,000 lines are sent, after a local prefilter. The prefilter drops blank lines and repeats. Past 1,500 distinct lines it keeps the neighbourhoods of error-like lines from the top, plus the tail of the log. Every line is clipped like `pick`'s.

## is

Ask a yes-or-no question about a text. Give `is` a statement and a text on stdin. It checks whether the statement is true of the text and answers with its exit code: 0 for yes, 1 for no, 3 for unsure. It prints nothing in human mode, so you can use it in a script like `test` or `grep -q`.

```
<stdin> | grevi is "<condition>" [--band 0.15]
```

| Flag | Meaning |
|:---|:---|
| `--band <0..=0.5>` | Unsure band around the threshold (default 0.15) |

With the defaults: yes at `p` ≥ 0.65, no below 0.35, unsure in between. `-t` moves the centre and `--band` the width. `--band 0` removes the unsure verdict. `data`: `p`, `verdict`, `truncated`.

```sh
grevi is "the customer is about to stop being a customer" < ticket.txt && ./page-account-manager
grevi is "asks for a refund" < mail.txt; case $? in 0) ./refund;; 1) ./archive;; 3) ./ask;; esac
grevi is --band 0.05 "is a stack trace" < crash.log
```

Write the statement literally. On the 24 support tickets in [benchmarks/agents/](../../benchmarks/agents/README.md), the first line above found the 6 churn risks and exited 3 on an employee who is leaving their company. The shorter "this customer is about to leave" says yes to that employee.

Limits: about 96,000 characters on TypeSafe and 30,000 on classifier. Longer input abstains before an API call: exit 3, `p:null`, `verdict:"unsure"`, `truncated:true`, and a reason; human mode warns on stderr. No verdict is inferred from a partial text. The model is not hardened against embedded instructions, so do not use `is` as a security gate on untrusted text. Counting, arithmetic, dates and general quality judgments are unreliable.

## run

Describe a task, get the command for it. Say what you want to do in plain English. `run` finds a tool on your machine that does it, takes the flags from that tool's man page, shows you the command, and asks before it runs it.

```
grevi run [--dry-run | --yes | --exec --yes] [--no-args] <intent...>
```

The request is the positional arguments joined with spaces; flags may follow it (`grevi run burn a dvd --dry-run`).

| Flag | Meaning |
|:---|:---|
| `--dry-run` | Only find the tool and show the command; never execute |
| `-y, --yes` | Run without asking |
| `--exec` | Allow execution in machine mode (requires `--yes`) |
| `--no-args` | Route only; do not point at flags or files |

Exit 0 when the command ran and exited 0, or, with `--dry-run`, when a tool was found. Exit 3 when no tool fits, 7 when the executed command failed, 130 when you declined at the prompt. `data`: `tool`, `fit`, `argv[]`, `flags[]`, `complete`, `blocked`, `executed`, `child_exit`, `alternatives[]`.

```sh
grevi run "burn a dvd from this iso"
grevi run --dry-run "count the lines in notes.txt"
grevi run --no-args "what's using port 8080"
grevi run --json --dry-run "extract foo.tar.gz"
grevi run --dry-run "keep my mac awake for an hour"   # caffeinate (0.87); `sleep` scores 0.10
```

A run has three steps, each one round of requests:

1. **Route.** grevi reads the tools on your PATH with the one-line summary of their man page, and asks Jev which tool answers the request.
2. **Fit.** It asks whether the proposed command is a correct, direct way to do the request. This is the answer that `-t` gates.
3. **Arguments.** It picks flags from that tool's man page, and file names in the current directory that contain a word of the request.

`--no-args` stops after step 2. Human mode prints a shell-quoted proposal. Only exact no-argument `true`, `false`, `pwd`, and `ls` forms can proceed to confirmation and execution; every other argv remains an unvalidated proposal. Without a TTY and without `--yes`, nothing executes.

Safety, always on:

- Commands run via argv, never through a shell. Flags come from man pages; no binary is ever probed with `--help`.
- Some tools are never executed, whatever the confidence: the destructive set (`rm`, `rmdir`, `dd`, `mkfs*`, `newfs*`, `fdisk`, `diskutil`, `shred`, `srm`, `wipefs`, `sudo`, `su`, `doas`, `kill`, `killall`, `pkill`, `reboot`, `halt`, `shutdown`, `poweroff`, `init`, `telinit`, `launchctl`, `systemctl`), the wrappers that would run another program named in their arguments (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `env`, `xargs`, `nohup`, `nice`, `timeout`, `time`, `exec`, `eval`, `command`, `find`, `watch`, `parallel`, `osascript`) and the interpreters that take program text as a flag value (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`, versioned names included). grevi shows the command, sets `data.blocked`, and leaves it to you. The list is by tool name only: `chmod -R` is not on it.
- In machine mode (`--json`) nothing executes unless `--exec --yes` is given, and the child's stdout goes to stderr so stdout stays one envelope.

`complete=false` means the argv has missing values or lacks a supported grammar; `blocked` explains the execution restriction. `--yes` and `--exec` do not bypass validation. The small supported set assumes trusted executables on PATH. Argument pointing is measured at 5 of 20 for "all required flags present" ([README, Numbers](../../README.md#numbers)); broader commands require your own operand, option and effect validation.

## add

Stage only the changes that belong to one topic. You fixed a bug and also cleaned up three other things. Name the fix, and `add` runs `git add` on the changes that belong to it and leaves the others unstaged. It does the job of `git add -p` without the questions. Git calls one such change a hunk, and so does `data`.

```
grevi add [--dry-run | --yes] "<topic>"
```

| Flag | Meaning |
|:---|:---|
| `--dry-run` | Score the hunks; stage nothing |
| `-y, --yes` | Stage without asking |

Exit 0 when hunks were staged, or scored with `--dry-run`. Exit 3 when no hunk is about the topic, 6 when there are no unstaged changes to tracked files, 130 when you declined. `data`: `hunks[{file, header, p, staged}]`.

```sh
grevi add --dry-run "the token expiry fix"
grevi add --yes "the token expiry fix" && git commit
```

Each full unstaged hunk of a tracked file is scored against the topic, and hunks at or above the threshold are staged. `add` can split changes in one file and works from any repository subdirectory. It touches the index only: no commits, untracked files or binary changes. Machine mode requires `--yes`; otherwise exit 130. A hunk over 3,000 characters (header plus body) is an input error before API calls or staging. Backend request budgets are also enforced; no unseen suffix is staged.

## sort

Tidy a messy folder. Point `sort` at a folder such as `~/Downloads`. It reads each file and proposes which of your existing subfolders the file belongs in. It moves nothing until you add `--apply`, and `--undo` moves everything back.

```
grevi sort <dir> [--into <root>] [--apply | --undo <log>]
```

| Flag | Meaning |
|:---|:---|
| `--into <root>` | Root whose sub-folders (depth ≤ 2) are the destinations (default: `<dir>`) |
| `--apply` | Move the files (dry run otherwise) and write an undo log |
| `--undo <log>` | Move files back using a log written by `--apply` |

Exit 0 with one proposed (or applied) move per file. Exit 3 when nothing can be placed or, with `--undo`, nothing was restored. Exit 6 when there are no folders under the root to sort into. `data`: `moves[{from, to, p}]`, `skipped[{file, reason}]`, `undo_log`, `applied`.

```sh
grevi sort ~/Downloads                      # dry run: shows where each file would go
grevi sort ~/Downloads --apply              # moves, writes an undo log
grevi sort ~/Downloads --undo <log>         # moves them back
grevi sort ~/Desktop --into ~/Documents     # files from one place, folders from another
```

`sort` reads the content, not the name. A file named `document(3).txt` that holds a 1099 tax form goes to `Taxes/2025`. A file that fits no folder stays where it is.

`sort` looks at the files directly in `<dir>`. It does not go into subfolders and it skips hidden files. For each file it asks Jev which of the existing folders under the root, up to two levels deep, the file belongs in. A file that the model cannot place, or places below the threshold, stays where it is and appears in `skipped`.

`sort` is a dry run by default. Apply/undo use atomic no-replace moves. A unique JSONL journal records durable intent before a move and completion afterward, retaining absolute Unix path bytes and file identity. Undo restores only a matching file to a free original path. Old TSV logs are rejected. Mid-run failures identify the recovery log and completed progress; undo failures are reported rather than silently omitted.

`sort` never replaces an occupied destination and never deletes files. It requires one volume and filesystem support for atomic no-replace operations. Source and destination symlink entries are skipped. Concurrent replacement of source files is unsupported. It sends eligible file/folder names and the first 2,000 characters of each text file after masking; PDFs use the first two pages through `pdftotext` when installed. This excerpt-based placement is not a claim to have read an entire document.

## Utility commands

### health

```sh
grevi health
```

Names the backend that answers, says whether a key was needed and accepted, and times the reply. Exit 0 ok, 4 unavailable, 5 auth. Makes no Jev request, so `meta.request_id` stays `null`.

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

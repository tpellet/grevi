# Getting started

## Install

With Rust 1.87 or newer, on macOS or Linux:

```sh
cargo install jevify --locked
```

The shell installer is also available:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tpellet/jevify/releases/latest/download/jevify-installer.sh | sh
```

Build from `main` with `cargo install --git https://github.com/tpellet/jevify --locked jevify`.

## No key needed

Without a key, jevify asks [classifier.dev](https://classifier.dev), which serves Jev free and
without an account: 20,000 classifications a day per IP. A TypeSafe key uses your own quota:

```sh
export TYPESAFE_API_KEY_FILE=/path/to/key
```

`TYPESAFE_API_KEY` works too. jevify reads the key file only when a request needs the key, and
never prints or logs it. `JEVIFY_BACKEND=typesafe|classifier` forces a backend. The two
backends give the same verbs and exit codes; their probabilities are not the same scale.

```sh
jevify health
```

`health` names the backend, whether it has a key, and the response time: exit 0 reachable,
4 unavailable, 5 missing or rejected TypeSafe key.

## First commands

The fixtures live in [docs/demo](../demo) of the repository; `bash docs/demo/examples.sh`
runs every example of the README.

Find the error in a failed build. On a live build, pipe both streams, since compilers write
errors to stderr: `cargo build 2>&1 | jevify why`.

```console
$ jevify why < docs/demo/build.log
jevify why: full output: ~/Library/Caches/jevify/outputs/1a419395094f905c.log
jevify why: 1812 lines, candidates 1212, windows 13
      1 │    Compiling buildfail v0.1.0 (benchmarks/fixtures/demo/buildfail)
>     2 │ error[E0425]: cannot find value `conifg` in this scope
      3 │    --> src/main.rs:306:20
      4 │     |
      5 │ 306 |     println!("{}", conifg);
```

Keep the lines where a statement holds, or pick the one line you describe:

```console
$ jevify filter 'reports a crash' < docs/demo/issues.txt
jevify filter: 10 records, 10 distinct, 1 requests
#312 Crash when the config file is empty
#290 Panic on non-UTF-8 file names
jevify filter: kept 2 of 10, 0 unsure, full output: ~/Library/Caches/jevify/outputs/c85c7cb6f33fc1f7.log

$ jevify pick "last month's electricity bill" < docs/demo/downloads.txt
jevify pick: candidates 8, windows 1
con_edison_electric_bill_august.pdf
```

Put a bucket in front of each line, then count the buckets with `cut`, `sort` and `uniq`:

```console
$ jevify label bug,feature,question < docs/demo/issues.txt | cut -f1 | sort | uniq -c
jevify label: 10 records, 10 distinct, 1 requests
jevify label: labelled 10 of 10, 0 unsure
   4 bug
   3 feature
   3 question
```

Ask a yes/no question and get the answer as an exit code:

```console
$ jevify is 'asks for a refund' < docs/demo/mail.txt && echo refund
refund
```

Find the file that does something, among the files of a repository:

```console
$ git ls-files | jevify pick --files 'where the command-line flags are defined'
jevify pick: candidates 319, windows 4
jevify pick: excerpts withheld: 0
src/cli.rs
```

When nothing fits, stdout stays empty and the exit code is 3:

```console
$ jevify pick 'the tax return' < docs/demo/downloads.txt
jevify pick: candidates 8, windows 1
$ echo $?
3
```

`why` and `filter` save their whole input, secrets included, under the cache directory's
`outputs/` and print the path on stderr. `--no-save` skips the save.

## Fill an argument

`fill` puts a real value where you wrote a description, then becomes the command. The
description goes in a marker, `@{kind:description}`, and the whole argument goes in single
quotes so that the shell leaves it alone.

```console
$ git log --oneline -30 | jevify fill --field 1 --dry-run -- git show --stat --format=%s '@{-:made route abstain when two commands are too close}'
jevify fill: - 317cbf7 0.99 (next 0.00, none 0.01) 317cbf7 fix: route abstains when two commands are too close (hunch-1zs); candidates 30, windows 1; model jev-1.13.0
jevify fill: would run 'git' 'show' '--stat' '--format=%s' '317cbf7'
'git' 'show' '--stat' '--format=%s' '317cbf7'
```

The status line on stderr gives the winner, its probability, the probabilities of the next
candidate and of "none of them", the evidence, the number of candidates and the model. This
run, on the keyless backend over 30 commits on 2026-09-22, answers 0.99 with the runner-up at
0.00 and "none of them" at 0.01. A winner resolves by standing clear of both, not by passing
the threshold on its own score — see
[what the threshold decides](how-it-works.md#what-the-threshold-decides).

Without `--dry-run`, `exec` names the command and the command owns everything after that: its
output, its exit code, your terminal. `--dry-run` prints the command instead of running it.
Look at it; never `eval` it.

```sh
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
printf 'retry_backoff\nparse_header\n' | jevify fill --dry-run -- cargo test '@{-:the test that retries a failed request}'
```

Three families of value exist. Things a tool can list: `branch`, `commit`, `file`, `dir`,
`tool`, `pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod`. Lines you pipe in:
`@{-:…}`. Options you write yourself, judged against a text on stdin or in `--context FILE`:
`'@{one:bug|feature|docs:what kind of report is this}'` picks one option, and
`'@{flag:--draft:the report lacks steps to reproduce}'` keeps the flag on yes, drops it on no,
and stops on doubt. Several markers resolve together; if one fails, nothing runs. stdin has one
role per call: the lines of `@{-:…}`, or the context of `one` and `flag`; the other side comes
from `--candidates FILE` or `--context FILE`.

`pick --from KIND` gives you the handle without a command:

```sh
jevify pick --from branch 'the auth refactor'
```

[Verbs](verbs.md#fill) covers escaping and the exact input rules; [Kinds](kinds.md) lists every
kind and how to add your own.

## The comma alias

`init` prints shell integration and edits no profile. The comma alias calls `route`, which
prints the installed tool for a task and starts nothing:

```sh
eval "$(jevify init zsh)"
, "what's using port 8080"
```

`jevify init bash` is the bash form. The zsh alias includes `noglob`. Quote requests with
apostrophes. With `JEVIFY_CNF=1` and no other handler, the snippet also passes unknown commands
of three or more words to `route`; shorter unknown commands return 127.

## Scripting on exit codes

Write the condition so that yes means act. One `is` statement prints nothing; several print
`VERDICT<TAB>STATEMENT` lines. `--context FILE` reads a file instead of stdin.

```sh
printf 'Please refund order 42.\n' | jevify is 'asks for a refund' 'mentions an order'
jevify is 'asks for a refund' --context mail.txt
```

Exit 0 means all yes, 1 at least one no, 3 unsure. `&&` acts on 0 only; use `case` when no,
unsure and backend errors need different handling. Check a `pick` call's exit code before its
output becomes an argument: on abstention the output is empty, and an empty argument is one
that many commands accept.

The codes are 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 reserved and
130 declined at the `add` confirmation. [Verbs](verbs.md) lists the data and flags of each
command. `--json` gives one machine envelope; [Agents](agents.md) describes it. `fill --json`
requires `--dry-run`. `fill` exits 2 to 6 when nothing ran; once the command runs, the exit
code is the command's. A dry run exits 0 when every marker resolves.

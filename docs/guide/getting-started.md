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

Without a key, jevify asks [classifier.dev](https://classifier.dev), which serves the free backend
without an account. A TypeSafe key selects TypeSafe and its quota:

```sh
export TYPESAFE_API_KEY_FILE=/path/to/key
```

`TYPESAFE_API_KEY` is also supported. jevify reads the key file only when it needs the key, and
never prints or logs the key. `JEVIFY_BACKEND=typesafe|classifier` forces a backend.
Scores and thresholds are not assumed interchangeable between the two backends.

```sh
jevify health
```

`health` names the backend, key status and response latency: exit 0 reachable, 4 unavailable,
5 missing or rejected TypeSafe key.

## First commands

```sh
printf 'build started\nerror: connection timed out\nbuild stopped\n' | jevify filter 'reports a network failure'
printf 'All tests passed.\n' | jevify is 'the tests passed' && printf 'ready\n'
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
jevify pick --from branch 'the auth refactor'
cargo build 2>&1 | jevify why
git branch | jevify pick 'the payment timeout fix'
git ls-files | jevify pick --files 'where man pages are parsed'
jevify route 'keep my mac awake for an hour'
```

`why` prints numbered lines with context. Compilers write errors to stderr, so pipe `2>&1`.
`pick` prints selected input records. `filter` keeps matching and unsure records, with `--strict`
to drop unsure ones. `filter -v` inverts; `--verbose` prints diagnostics and has no short flag.

Only `why` and `filter` save full raw input, secrets included, under the cache directory's
`outputs/` subdirectory. Stderr names the saved path. `--no-save` skips the save independently
of the answer cache; a skipped or failed save sets `data.complete=false`.

## Fill an argument

`fill` selects existing handles and becomes the command you wrote. `--dry-run` prints its argv
without executing; inspect the preview, never `eval` it. Omit `--dry-run` only when the command
is authorized. Several markers are all-or-nothing: if any abstains, nothing runs.

```sh
printf 'retry_backoff\nparse_header\n' | jevify fill --dry-run -- cargo test '@{-:the retry test}'
```

Single-quote the whole marker argument. Three families supply values: existing branches with
`branch`, supplied records with `-`, or caller-written options with `one` and `flag`.
`'@{one:bug|feature|docs:what kind of report is this}'` chooses an option from context;
`'@{flag:--draft:the report lacks steps to reproduce}'` keeps or removes a whole flag argument,
abstaining on doubt. Context comes from stdin or `--context FILE`; candidates come from stdin
or `--candidates FILE`. stdin cannot serve both roles. File inputs leave stdin for the command.

Use `pick --from branch` for the handle without execution. Use cheap literal tools when you
already know its name. [Verbs](verbs.md#fill) covers marker escaping and the exact input rules.

## The comma alias

`init` prints shell integration; it does not edit a shell profile. The comma alias and opt-in
command-not-found hook call `route`, which prints a tool and starts no user command.

```sh
eval "$(jevify init zsh)"
, "what's using port 8080"
```

Use `jevify init bash` for bash. The zsh alias includes `noglob`. Quote requests with apostrophes.
When `JEVIFY_CNF=1` is set and no handler exists, the snippet defines a hook for unknown commands
of three or more words. Shorter unknown commands return 127.

## Scripting on exit codes

Write the condition so that yes means act. One `is` statement prints nothing; several print
`VERDICT<TAB>STATEMENT` lines. `--context FILE` uses a file instead of stdin.

```sh
printf 'Please refund order 42.\n' | jevify is 'asks for a refund' 'mentions an order'
jevify is 'asks for a refund' --context mail.txt
```

Exit 0 means all yes, 1 means at least one no, and 3 means unsure otherwise. `&&` acts only on 0.
Use `case` to distinguish no, unsure and backend errors. Check a `pick` call's exit before using
its output as an argument; an unchecked substitution can pass an empty argument on abstention.

The common codes are 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 reserved
and 130 declined at `add` confirmation. [Verbs](verbs.md) lists the per-command data and flags.
For machine output, use `--json`; [Agents](agents.md) describes the envelope.
`fill --json` requires `--dry-run`. Before execution, exits 2–6 mean nothing ran; after execution,
the command owns its exit code. A dry run exits 0 when all markers resolve.

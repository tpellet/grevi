# Getting started

## Install

With Rust 1.87 or newer:

```sh
cargo install grevi --locked
```

Or the shell installer, macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tpellet/grevi/releases/download/v0.2.0/grevi-installer.sh | sh
```

To build the unreleased `main` instead: `cargo install --git https://github.com/tpellet/grevi --locked grevi`.

## Get a key

grevi is an independent open-source client of TypeSafe's hosted Jev API; you need your own key from https://console.typesafe.ai. Requests are billed to your key.

Give grevi the key one of two ways:

```sh
export TYPESAFE_API_KEY=...
# or, to keep it out of your environment and shell history:
export TYPESAFE_API_KEY_FILE=/path/to/key
```

`TYPESAFE_API_KEY_FILE` is read only when a request needs a key. grevi never prints the key and never logs it.

## Check the connection

```sh
grevi health
```

`health` reports whether the key is accepted and how long the API took to answer. It exits 0 when both are fine, 4 when the API is unreachable and 5 when the key is missing or rejected.

## First commands

```sh
cargo build 2>&1 | grevi why                       # the line that broke the build
ls ~/Downloads | grevi pick "last month's electricity bill"
grevi is "asks for a refund" < mail.txt && ./refund
grevi run --dry-run "count the lines in notes.txt" # proposes `wc -l notes.txt`, runs nothing
```

Three things to know before going further:

- Compilers write errors to stderr. Pipe `2>&1` into `why`, or let `grevi why -- cargo build` run the command and capture both streams.
- Exit 3 is an answer, not an error. It means nothing fit (`pick`, `run`), no line looked like a failure (`why`), or the yes/no probability landed in the unsure band (`is`). Branch on it.
- Every verb makes at least one API request. `-v` prints the request count, the probabilities and the cost on stderr.

## The `,` alias

`grevi init` prints a snippet that makes `,` an alias for `grevi run`:

```sh
eval "$(grevi init zsh)"      # or bash; add the line to ~/.zshrc or ~/.bashrc
, "what's using port 8080"
```

Quote requests that contain an apostrophe: an unquoted `, what's using port 8080` opens a quote in both zsh and bash. In zsh the alias is `noglob grevi run`, so `*` in a request is not expanded.

The same snippet holds an opt-in command-not-found hook. With `GREVI_CNF=1` in the environment, a command the shell cannot find that has three or more words is passed to `grevi run` instead of failing with "command not found". Shorter unknown commands still fail as before. grevi never installs the hook on its own: it is only defined if you export the variable and no handler exists already.

## Scripting on exit codes

`is` prints nothing: the answer is the exit code, so it composes with `&&`, `||` and `case`.

```sh
grevi is "asks for a refund" < mail.txt; case $? in 0) ./refund;; 1) ./archive;; 3) ./ask;; esac
```

`pick` and `why` print the matching line on stdout, so they sit inside `$( )`:

```sh
git switch $(git branch | grevi pick "payment timeout fix")
kill $(ps -eo pid,comm,%cpu | grevi pick "eating my battery" | awk '{print $1}')
```

The exit codes, common to every verb: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined. [Verbs](verbs.md) lists which ones each verb can return.

## Next

- [Verbs](verbs.md) for every flag and the `data` each verb returns.
- [Agents](agents.md) if a program, not a person, will read the output.
- [Configuration](configuration.md) for the environment variables.

# jevify

jevify finds the thing you can describe but cannot name: the error in a long build, the commit
that did something, the file that does something, the issues that report a crash. It looks only
at things that exist, prints the one that fits, and tells you when none does. Under it is
[Jev](https://docs.typesafe.ai), a small model from [TypeSafe AI](https://typesafe.ai) that
answers "which one", "is it true" and "how much" with a probability, and writes no text.

[![CI](https://github.com/tpellet/jevify/actions/workflows/ci.yml/badge.svg)](https://github.com/tpellet/jevify/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/jevify)](https://crates.io/crates/jevify)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The examples run on the fixtures in [docs/demo](docs/demo) and on this repository's own
history; `bash docs/demo/examples.sh` runs them all.

## Find the error in a long build

Your build fails with 1,812 lines of output, 300 of them warnings. `why` reads all of it and
prints the line that explains the failure, with the lines around it.

![jevify why finds the one error in 1,812 lines of cargo output](docs/img/why.svg)

```console
$ jevify why < docs/demo/build.log
jevify why: full output: ~/Library/Caches/jevify/outputs/1a419395094f905c.log
jevify why: 1812 lines, candidates 1212, windows 7
      1 │    Compiling buildfail v0.1.0 (benchmarks/fixtures/demo/buildfail)
>     2 │ error[E0425]: cannot find value `conifg` in this scope
      3 │    --> src/main.rs:306:20
      4 │     |
      5 │ 306 |     println!("{}", conifg);
```

On a live build, pipe both streams: `cargo build 2>&1 | jevify why`. The whole log is saved
and its path printed, so you can always go back to it.

## Run git on the commit you can describe

You know what a commit did, not its hash. Write the git command as usual and put
`'@{commit:what it did}'` where the hash goes. jevify lists the commits, picks the one that
fits, and hands the command to git.

![jevify fill turns a description of a commit into the real commit, then runs git show on it](docs/img/fill.svg)

```console
$ jevify fill -- git show --stat --format=%s '@{commit:stopped sending the free backend batches it refuses}'
jevify fill: commit 7bcf70cd91fc3d9e306d60ea0436a02983be3d6f 0.96 (next 0.02, none 0.02) fix: keyless batches of at most 60 records, 402 named (hunch-0it); candidates 173, windows 1; model jev-1.13.0
jevify fill: exec 'git' 'show' '--stat' '--format=%s' '7bcf70cd91fc3d9e306d60ea0436a02983be3d6f'
fix: keyless batches of at most 60 records, 402 named (hunch-0it)

 CHANGELOG.md                |  8 ++++++++
 docs/ROBOT_MODE.md          |  2 +-
 …
 11 files changed, 97 insertions(+), 38 deletions(-)
```

The same marker works for a branch, a file, a directory, a PR, an issue, a CI run, a stash, a
process, a container or a pod: `'@{branch:the auth refactor}'`, `'src/@{file:parses the marker}'`,
`'@{pod:the payment worker}'`. Anything a tool can list works through a pipe:
`cargo test -- --list | sed 's/: test$//' | jevify fill -- cargo test '@{-:the retry test}'`.
`--dry-run` prints the command it would run and runs nothing.

```console
$ jevify fill --dry-run -- cat 'src/@{file:reads the recipes of the kinds}'
jevify fill: file kinds.jsonl 0.63 (next 0.02, none 0.19) kinds.jsonl; candidates 32, windows 1; model jev-1.13.0
jevify fill: exec 'cat' 'src/kinds.jsonl'
'cat' 'src/kinds.jsonl'
```

## Sort issues into buckets

Ten issue titles, three buckets, one request. `label` puts the bucket and a tab in front of
each line, so `cut`, `sort` and `uniq` count them as they count anything.

![jevify label tags ten issue titles as bug, feature or question, and a pipe counts them](docs/img/label.svg)

```console
$ jevify label bug,feature,question < docs/demo/issues.txt
jevify label: 10 records, 10 distinct, 1 requests
bug	#312 Crash when the config file is empty
feature	#309 Add a dark theme to the settings page
question	#305 How do I run this behind a corporate proxy?
bug	#301 Export to CSV drops the last row
feature	#298 Support Windows paths in the installer
question	#294 Is there a way to disable telemetry?
bug	#290 Panic on non-UTF-8 file names
feature	#287 Keyboard shortcut to jump to the next match
question	#283 Does the free plan include the API?
bug	#281 Memory grows without bound on long sessions
jevify label: labelled 10 of 10, 0 unsure

$ jevify label bug,feature,question < docs/demo/issues.txt | cut -f1 | sort | uniq -c
   4 bug
   3 feature
   3 question
```

`filter` keeps the lines where a statement holds, and `pick` returns the one line you describe:

```console
$ jevify filter 'reports a crash' < docs/demo/issues.txt
jevify filter: 10 records, 10 distinct, 1 requests
#312 Crash when the config file is empty
#290 Panic on non-UTF-8 file names
jevify filter: kept 2 of 10, 0 unsure, full output: ~/Library/Caches/jevify/outputs/c85c7cb6f33fc1f7.log
```

## It says no when nothing fits

Eight file names. The electricity bill is there; the tax return is not. A question about
something that is not there gets nothing on stdout and exit code 3, so a script stops instead
of acting on the best of a bad list.

![jevify pick finds the electricity bill among eight downloads, then refuses to guess a tax return that is not there](docs/img/nothing-fits.svg)

```console
$ jevify pick "last month's electricity bill" < docs/demo/downloads.txt
con_edison_electric_bill_august.pdf

$ jevify pick 'the tax return' < docs/demo/downloads.txt
$ echo $?
3
```

The same holds for `fill`: if the description fits no commit, no command runs.

## Branch a script on a fact

`is` answers a yes/no question about its input with an exit code, like `test`, so it goes
straight into `&&`, `if` and `until`.

```console
$ jevify is 'asks for a refund' < docs/demo/mail.txt && echo refund
refund
```

```sh
until kubectl get pods | jevify is 'every pod is ready'; do sleep 5; done
cargo test 2>&1 | jevify is 'every failure is a network timeout' && cargo test
```

## Try it

With Rust 1.87 or newer, on macOS or Linux:

```sh
cargo install jevify --locked
```

No key and no account: [classifier.dev](https://classifier.dev) answers 20,000 classifications
a day per IP for free. A TypeSafe key uses your own quota instead:
`export TYPESAFE_API_KEY_FILE=/path/to/key`. `jevify health` tells you which backend answers.

Three commands that work in any repository:

```sh
printf 'build started\nerror: connection timed out\nbuild stopped\n' | jevify filter 'reports a network failure'
git ls-files | jevify pick --files 'where the command-line flags are defined'
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
```

## The verbs

| You want | Command | Like |
|:---|:---|:---|
| the line that explains a failure | `jevify why < log` | |
| one line out of many | `jevify pick 'description' < lines` | `fzf --filter` |
| only the lines that matter | `jevify filter 'statement' < lines` | `grep` |
| a bucket for each line | `jevify label a,b,c < lines` | an `awk` key |
| a yes or no to act on | `jevify is 'statement' < text` | `test` |
| a command run on the thing you describe | `jevify fill -- CMD '@{kind:description}'` | |
| the handle alone, no command | `jevify pick --from KIND 'description'` | |
| the installed tool for a task | `jevify route 'task'` | `apropos` |

`pick`, `filter` and `label` read lines; `--para` reads paragraphs and `-0` NUL-separated
records; `--files` reads paths and judges the first lines of each file. `pick` and `filter`
print input lines unchanged, in input order. `why` and `filter` save their whole input and
print its path.

`fill` takes any command after `--`. A marker is `@{kind:description}`, and the whole argument
goes in single quotes so that the shell leaves it alone. The kinds are `branch`, `commit`,
`file`, `dir`, `tool`, `pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod`, `-` for
lines on stdin, and two that judge a text instead of listing things:
`'@{one:bug|feature|docs:what kind of report is this}'` picks one of the options you wrote,
and `'@{flag:--draft:the report lacks steps to reproduce}'` keeps or drops a flag. A kind is one
line of JSON naming the command that lists and the field that is the handle; your own go in
`kinds.jsonl` in the configuration directory. Several markers resolve together; if one fails,
nothing runs.

```sh
jevify fill --dry-run -- git revert '@{commit:made folder moves atomic}'
printf 'A crash with no reproduction steps.\n' | jevify fill --dry-run -- printf '%s\n' \
  '@{one:bug|feature|docs:what kind of report is this}' '@{flag:--draft:the report lacks steps to reproduce}'
jevify pick --from branch 'the auth refactor'
```

## How it answers

Code does the listing: `git log` for commits, `git ls-files` for files, your pipe for lines.
Each candidate comes with evidence, such as a commit's subject and changed paths or a file's
first lines. Jev gets the description, the candidates and one more option, "none of them", and
returns a probability for each. A long list is judged in windows of 200 candidates (99 without
a key), all at the same time, and the best of each window meet in one final comparison, so a
selection takes at most two rounds. jevify prints the answer on stdout and the probabilities on
stderr, and prints no candidate it is unsure about: the winner has to lead the runner-up
clearly.

Jev writes nothing, so jevify invents nothing: every value it prints already existed. It does
not count, compute or compare dates; `sort`, `awk` and `jq` do that before or after it. A text
under judgment can argue with the judge, so keep security decisions out of it.

## Exit codes

Write the condition so that yes means act; `&&` acts on 0 only.

| Code | Meaning |
|---:|:---|
| 0 | yes, found, done |
| 1 | no (`is`), nothing kept (`filter`) |
| 2 | usage error |
| 3 | nothing fits, or unsure |
| 4 | backend unavailable or daily quota reached |
| 5 | TypeSafe key missing or rejected |
| 6 | empty, oversized or unreadable input |
| 130 | declined at the `add` confirmation |

`fill` exits 2 to 6 when nothing ran; once the command runs, its exit code is the command's.

## For agents

`jevify init agents` prints a block for your `AGENTS.md`: each situation with its complete
command. The [skill](plugins/jevify/skills/jevify/SKILL.md) teaches the same to Claude Code
(install the `tpellet/jevify` marketplace and the `jevify@jevify` plugin) and to Codex (copy
the skill directory into `~/.agents/skills/`). Every command takes `--json` and answers with one envelope,
`{ok, command, version, exit_code, data, meta, error}`; `jevify capabilities --json` lists every
command, flag, kind and limit. Allow `jevify fill --dry-run` freely and `fill` per command
prefix, `jevify fill -- git switch:*`, exactly as you allow the command itself.

## Reference

- [Getting started](docs/guide/getting-started.md): install, backends, first commands
- [Verbs](docs/guide/verbs.md): every flag, every data field, every exit code per verb
- [Kinds](docs/guide/kinds.md): the kinds, the recipe line, `kinds.jsonl`, the status line
- [Agents](docs/guide/agents.md) and the [robot mode contract](docs/ROBOT_MODE.md): the envelope
- [How it works](docs/guide/how-it-works.md): selection, thresholds, measurements
- [Configuration](docs/guide/configuration.md): variables, limits, where files live
- [FAQ](docs/guide/faq.md), [Privacy](PRIVACY.md), [Changelog](CHANGELOG.md)

`add` stages the hunks that belong to a topic (`jevify add --dry-run 'the token expiry fix'`),
`sort` proposes a folder for each file of a directory (`jevify sort ~/Downloads`), and
`eval "$(jevify init zsh)"` gives you `, "what's using port 8080"` for `route`.

## Privacy and limits

Requests go to the backend you chose, with the description and the evidence and nothing else;
[PRIVACY.md](PRIVACY.md) lists what each verb sends. Answers are cached for seven days
(`--no-cache`). `why` and `filter` save their raw input under the cache directory, secrets
included, until you delete it (`--no-save`).

Without a key, one classification is one record under one question, and a day holds 20,000
per IP: 20,000 records through `filter` or `label`, at most 60 records per request; 830 to
10,000 `pick` or `why` calls, from 1,000 lines down to 99; about 380 `route` calls over a PATH
of 1,900 commands (measured 2026-09-22, [benchmarks/results.md](benchmarks/results.md)). A verb
takes 20,000 distinct records at most, and `fill` 3,267 candidates per marker without a key or
13,200 with one. A probability is the backend's score, not a promise; the
[measurements](docs/guide/how-it-works.md#numbers) say where it was checked.

## Credits and license

Powered by Jev from TypeSafe AI, with keyless access through classifier.dev. Development tooling
includes Jeffrey Emanuel's [Agent Mail](https://github.com/Dicklesworthstone/mcp_agent_mail)
and [beads](https://github.com/Dicklesworthstone/beads_rust).

MIT.

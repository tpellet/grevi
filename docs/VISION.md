# jevify vision

jevify gives existing command-line tools an understanding of meaning. An agent knows which tool
to call. It often does not know the exact name to pass, and it often gets more output than it
needs. jevify works at those two borders of a command and leaves the command itself alone.

## What the model can and cannot do

Jev answers three kinds of question about a text: which one of these options, is this true, and
how much. It returns a probability with each answer. It writes no text.

Three rules follow.

- **jevify selects, it never invents.** Every value it prints already exists: a path, a branch, a
  line, a label the caller supplied. Code lists the candidates; the model picks among them.
- **The work is in the candidates.** A good answer needs a complete list and enough evidence for
  each item: a branch with its last commit subject, a file with an excerpt, a pod with its status.
- **"Nothing fits" is an answer.** When no candidate fits or two are too close, jevify says so
  with exit code 3 and changes nothing.

## Two sides of a command

```
                 input side                            output side
   jevify fill -- git switch @{branch:'…'}    cargo test 2>&1 | jevify why
   a description becomes a real argument      a wall of output becomes an answer
```

### The input side: `fill`

The agent knows the command and the operation. It does not know the handle: which branch, which
file, which commit, which pod, which process. Without jevify it lists, reads the listing, chooses,
and then acts. That is two or three turns, and the whole listing enters its context.

```
jevify fill -- git switch @{branch:'the auth refactor'}
jevify fill -- git revert @{commit:'made folder moves atomic'}
jevify fill -- cargo test @{test:'the retry backoff cap'}
jevify fill -- mkdir @{dir:'where the release scripts live'}/archive
jevify fill -- kubectl --context=@{complete:'the staging cluster'} get pods
gh pr list --json number,title | jevify fill --key number -- gh pr view @{-:'the Windows path fix'}
```

**The marker.** `@{kind:'description'}` stands anywhere inside one argument: alone, before a
suffix, after a prefix, or as a flag value. The quotes keep the shell from splitting or expanding
the text. The kind names a kind of thing, not a command.

- An unknown kind is a usage error, exit 2, so a typing mistake never reaches the tool.
- `@{u}`, `@types/node` and `user@host` hold no `@{word:` and stay literal. `@@{` writes a
  literal `@{`.
- Several markers resolve at the same time against one snapshot. If one abstains, nothing runs.
- Everything that is not a marker passes through byte for byte. jevify never parses the tool's
  flags and keeps no table of commands.

**The candidates.** Each kind lists its own candidates and attaches the evidence that tells them
apart. A selection reports what it looked at: the scope, the number of candidates, and anything
left out. A high rank never proves that only one candidate fits.

| Kind | Candidates | Evidence |
|:---|:---|:---|
| `-` | records on stdin, or `--candidates FILE` | the record; `--key` names the handle in JSON, `--field` in lines |
| `branch` | local and remote refs | name, last commit subject, age |
| `commit` | the log of the current branch | subject, body, changed paths |
| `test`, `script`, `target` | the test runner, `package.json`, `make`, `just` | the identifier and its description |
| `pr`, `issue`, `ci-run` | `gh` | title, state, branch |
| `file`, `dir` | tracked and untracked files that are not ignored, hidden ones included | path, first lines |
| `stash`, `process`, `container`, `pod`, `host` | the owning tool | name and status |
| `complete` | the tool's own completion protocol, at the marker's position | the completion and its description |

- `@{-:…}` works with any tool that prints a list, with no built-in knowledge. It owns stdin, so
  the command receives an empty stdin; `--candidates FILE` leaves stdin to the command.
- A literal prefix narrows a list: `src/cmd/@{file:'stages hunks'}` looks only under `src/cmd/`.
- Candidates come from the directory jevify runs in. `fill -C DIR` changes it. jevify never reads
  the scope out of the tool's own flags.
- When two candidates are close, `fill` fetches richer evidence once, then abstains.
- Code handles order, counts and dates before the model is asked. "The newest branch" is a sort.

**The result.** `fill` prints the resolved command on stdout, quoted for the shell, and the
evidence for each choice on stderr. `--run` executes it directly, with no shell in between.

- The caller's own permission system decides what may run; jevify is not a permission system.
- With `--run`, stdout and stderr belong to the command. jevify adds one stderr line,
  `jevify fill: ran, exit N`, and exits 0 when the command succeeded and 7 when it failed.
- When nothing runs, stdout is empty, stderr says `jevify fill: not run:` with the reason, and
  the exit code is 2, 3, 4, 5 or 6. Exit 3 names `no_match`, `ambiguous` or
  `insufficient_evidence`.
- Right before it runs a command, jevify checks that each chosen thing is still the same.
- New values stay literal. `fill` resolves references to things that exist: the parent directory
  of a new folder, the source of a copy, the branch to rebase onto.
- `fill --run` is the only place where jevify starts a command.

The agent uses it because one call replaces list, read, choose, and the listing never enters its
context. It gains most where the listing is long and the names are opaque: commits, branches,
tests, CI runs. It gains nothing for a name the agent has already seen or a string that a literal
search finds.

### The output side: `why`, `pick`, `filter`, `label`, `is`

The command ran. Its output is long, or it needs a judgment before the next step.

| The agent wants | Verb | Returns |
|:---|:---|:---|
| the line that explains a failure | `why` | one line with context |
| one record out of many | `pick 'description'` | one record |
| only the records that matter | `filter 'statement'` | a subset, and the path of the full output |
| a tag on each record, to route or triage | `label a,b,c` | each record with its label |
| a decision to branch on | `is 'statement'` | an exit code |

- Output verbs read stdin and never start a command. They send the evidence to the configured
  backend and may save the input on this machine; they do nothing else.
- A record is a line. `--para` makes it a block between blank lines, for test failures and stack
  traces. `--key` reads a JSON array or JSON lines. `--files` reads paths and judges each file's
  first lines. Records come out unchanged and in their input order.
- A reduction always leaves a way back: the full input is saved and its path is printed.
  `filter` keeps what it is unsure about and says so: `kept 31 of 10074, 2 unsure, full output:
  PATH`. `--strict` drops the unsure ones. A reduction that could not save its input says so and
  never claims to be complete.
- `is 'a' 'b' 'c'` asks several statements in one request and prints one verdict per line. It
  exits 0 when all hold, 1 when one does not, 3 otherwise.
- `label --ordered low,mid,high` places each record on a scale.
- A line that starts with `jevify fill:` is never a record, so `fill --run … 2>&1 | jevify why`
  reads only the command's output. When the input is empty, an output verb exits 6; the reason
  is the `not run` line that `fill` wrote to stderr. A script that must stop there sets
  `pipefail`.

The agent uses them because it reads thirty lines where it would read ten thousand, and because
a yes or no in an exit code costs no model turn.

## One grammar

```
jevify VERB [options] ['text'] [-- COMMAND ARGS…]
```

The first word is always a jevify verb. Only `fill` takes a command. There is no bare
`jevify COMMAND` form, no guessing whether an argument is a description, and no second spelling
of a verb as a flag.

Exit codes are the contract: 0 yes or found, 1 no, 3 abstain, and the others as `AGENTS.md`
lists them. Candidates and probabilities go to stderr or to the `--json` envelope, never to
stdout where they could become an argument.

## What jevify is not

- It does not choose the tool. A frontier model knows the common tools; `run` stays for the long
  tail of a large PATH and is not the headline.
- It does not generate commands, flags, messages or file names.
- It does not count, calculate or compare dates. Code does that before the model is asked.
- It is not a security gate. Text under judgment can argue with the judge.

## Where an answer is worth having

jevify earns its call where checking the answer is cheaper than doing the work. A pointer to one
line, one branch or one file is checked at a glance. A subset is checked only by reading the
whole input, so `filter` always keeps the full output within reach.

The skill file is part of the product. An agent reaches for a tool only when it knows the
situations that call for it: a name it can describe and cannot spell, an output too long to read,
many records and one question.

# jevify vision

jevify gives existing command-line tools an understanding of meaning. An agent knows which tool
to call. It often does not know the exact value to pass, and it often gets more output than it
needs. jevify works at those two borders of a command and leaves the command itself alone.

## What the model can and cannot do

Jev answers three kinds of question about a text: which one of these options, is this true, and
how much. It returns a probability with each answer. It writes no text.

Three rules follow.

- **jevify selects, it never invents.** Every value it prints already exists: a path, a branch, a
  line, an option the caller supplied. Code lists the candidates; the model picks among them.
- **The work is in the candidates.** A good answer needs a complete list and enough evidence for
  each item: a branch with its last commit subject, a file with an excerpt, a pod with its status.
- **"Nothing fits" is an answer.** When no candidate fits or two are too close, jevify says so
  with exit code 3 and changes nothing.

## Two sides of a command

```
                 input side                              output side
   jevify fill -- git switch '@{branch:…}'      cargo test 2>&1 | jevify why
   a description becomes a real argument        a wall of output becomes an answer
```

### The input side: `fill`

The agent knows the command and the operation. It does not know a value: which branch, which
commit, which test, which option fits this ticket. Without jevify it lists, reads the listing,
chooses, and then acts. That is two or three turns, and the whole listing enters its context.

`fill` fills any argument whose value can be listed. It runs the command it resolved.

```
jevify fill -- git switch '@{branch:the auth refactor}'
jevify fill -- git revert '@{commit:made folder moves atomic}'
jevify fill -- cargo test '@{test:the retry backoff cap}'
jevify fill -- mkdir '@{dir:where the release scripts live}/archive'
jevify fill -- kubectl '--context=@{complete:the staging cluster}' get pods
gh pr list --json number,title | jevify fill --key number -- gh pr view '@{-:the Windows path fix}'
cargo build 2>&1 | jevify fill -- "$EDITOR" '@{-:the file that fails to compile}'
```

Three families of value can be listed.

| Family | Where the candidates come from | Marker |
|:---|:---|:---|
| things that exist | a lister per kind | `@{branch:…}`, `@{commit:…}`, `@{test:…}`, `@{file:…}` |
| options the tool accepts | the tool's own completions, or a set the caller writes | `@{complete:…}`, `@{one:a\|b\|c:…}`, `@{flag:--draft:…}` |
| values present in a context | records or spans that code extracts from stdin or a file | `@{-:…}` |

Several markers fill several arguments in one call. Markers that judge the same context share
one request, so three arguments cost about as much as one:

```
cat ticket.md | jevify fill -- gh issue create --title 'Crash on empty input' \
    '--label=@{one:bug|feature|docs:what kind of report is this}' \
    '--assignee=@{one:ana|raj|kim:who owns the affected area}' \
    '@{flag:--draft:the report lacks steps to reproduce}'
```

Free text stays literal: a title, a commit message, a new file name, a pattern. The agent writes
those; jevify fills what needs a lookup or a judgment.

**The marker.** `@{kind:description}` stands anywhere inside one argument: alone, before a
suffix, after a prefix, or as a flag value. The whole argument goes in single quotes, with no
quotes inside, so that no shell splits, expands or globs it.

- The kind is `-` or a registered lowercase word. The description ends at the first `}`; `\}`
  writes a literal one. One pair of quotes around the description is dropped, for callers that
  pass an argument list and no shell.
- `@{one:a|b|c:question}` lists its options up to the first `:` and splits them on `|`. `\:`,
  `\|` and `\}` are the escapes.
- `@{flag:--name:question}` is a whole argument. A yes leaves `--name`; a no removes the
  argument.
- An unknown kind, a marker with no closing `}`, and a `fill` with no marker at all are usage
  errors, exit 2, and nothing runs. The error names the nearest kind. A marker that a shell
  damaged never reaches the tool.
- `@{u}`, `HEAD@{2}`, `stash@{0}`, `@types/node` and `user@host:path` hold no `@{kind:` and stay
  literal. `@@{` writes a literal `@{`.
- Everything that is not a marker passes through byte for byte. jevify never parses the tool's
  flags and keeps no table of commands.

**The candidates.** Each kind lists its own candidates and attaches the evidence that tells them
apart. A selection reports what it looked at: the scope, the number of candidates, and anything
left out. A high rank never proves that only one candidate fits, and jevify never picks the
first of several silently.

| Kind | Candidates | Evidence |
|:---|:---|:---|
| `-` | records on stdin, or `--candidates FILE` | `--key` or `--field` names the handle, `--evidence` the fields the model reads, `-0` reads NUL-separated records |
| `branch` | local and remote refs | name, last commit subject, age |
| `commit` | the log of the current branch | subject, body, changed paths |
| `test`, `script`, `target` | the test runner, `package.json`, `make`, `just` | the identifier and its description |
| `pr`, `issue`, `ci-run` | `gh` | title, state, branch |
| `file`, `dir` | tracked and untracked files that are not ignored, hidden ones included | path, first lines |
| `stash`, `process`, `container`, `pod`, `host` | the owning tool | name and status |
| `tool` | the commands on the PATH | name and one-line manual summary |
| `complete` | the tool's own completion protocol, at the marker's position | the completion and its description |

- A literal prefix narrows a list: `'src/cmd/@{file:stages hunks}'` looks only under `src/cmd/`.
- Candidates come from the directory jevify runs in. `fill -C DIR` changes it. jevify never reads
  the scope out of the tool's own flags.
- `one` and `flag` judge a context: stdin, or `--context FILE`.
- Markers resolve at the same time against one snapshot. `complete` waits for the arguments on
  its left. If one marker abstains, nothing runs.
- When two candidates are close, `fill` fetches richer evidence once, then abstains.
- Code handles order, counts and dates before the model is asked. "The newest branch" is a sort.

**The run.** `fill` executes the resolved argument list directly, with no shell in between.
`--dry-run` prints the command on one line, quoted so that bash, zsh and dash read it back
exactly, and runs nothing. The printed line is the command that a run executes.

- The caller's own permission system decides what may run; jevify is not a permission system.
- stdout and stderr belong to the command. jevify writes to stderr only, every line starts with
  `jevify fill:`, and the last one is `ran, exit N`, `ran, signal N` or `not run:` with a reason.
- Exit 0 when the command succeeded, 7 when it failed, 2 to 6 when nothing ran. Exit 3 names
  `no_match`, `ambiguous` or `insufficient_evidence`. `fill` forwards interrupt and termination
  signals to the command.
- A marker that reads stdin takes all of it, and the command receives an empty stdin.
  `--candidates FILE` and `--context FILE` leave stdin to the command. With no such marker the
  command inherits stdin and the terminal.
- Right before it runs a command, jevify checks that each chosen thing is still the same.
- `fill` is the only place where jevify starts a command.

The agent uses it because one call replaces list, read, choose and act, and the listing never
enters its context. It gains most where the listing is long and the names are opaque: commits,
branches, tests, CI runs; and where an option depends on text the agent has not read. It gains
nothing for a name the agent has already seen or a string that a literal search finds.

### The output side: `why`, `pick`, `filter`, `label`, `is`

The command ran. Its output is long, or it needs a judgment before the next step.

| The agent wants | Verb | Returns |
|:---|:---|:---|
| the line that explains a failure | `why` | one line with context |
| one record out of many | `pick 'description'` | one record |
| only the records that matter | `filter 'statement'` | a subset, and the path of the full output |
| a tag on each record, to sort or triage them | `label a,b,c` | each record with its label |
| a decision to branch on | `is 'statement'` | an exit code |

- Output verbs read stdin and never start a command. They send the evidence to the configured
  backend and may save the input on this machine; they do nothing else.
- A record is a line. `--para` makes it a block between blank lines, for test failures and stack
  traces. `--key` reads a JSON array or JSON lines. `-0` reads NUL-separated records. `--files`
  reads paths and judges each file's first lines. Records come out unchanged and in their input
  order.
- `pick --from KIND 'description'` selects among a kind's candidates in place of stdin and prints
  the handle. `route 'task'` is `pick --from tool`: it names the installed tool for a task, with
  its summary and synopsis, and runs nothing. The agent writes the command.
- A reduction always leaves a way back: the full input is saved and its path is printed.
  `filter` keeps what it is unsure about and says so: `kept 31 of 10074, 2 unsure, full output:
  PATH`. `--strict` drops the unsure ones. A reduction that could not save its input says so and
  never claims to be complete.
- `is 'a' 'b' 'c'` asks several statements in one request and prints one verdict per line. It
  exits 0 when all hold, 1 when one does not, 3 otherwise.
- `label --ordered low,mid,high` places each record on a scale.
- The status lines of `fill` are never records, so `jevify fill -- CMD 2>&1 | jevify why` reads
  only the command's output. An output verb that receives only `not run` exits 3. A script that
  must stop on the first stage sets `pipefail`.

The agent uses them because it reads thirty lines where it would read ten thousand, and because
a yes or no in an exit code costs no model turn.

## One grammar

```
jevify VERB [options] ['text'…] [-- COMMAND ARGS…]
```

The first word is always a jevify verb. Only `fill` takes a command, and the command comes after
`--`. There is no bare `jevify COMMAND` form, no guessing whether an argument is a description,
and no second spelling of a verb as a flag. Text that starts with `-` goes after `--`.

Exit codes are the contract: 0 yes or found, 1 no, 2 usage, 3 abstain, and the others as
`AGENTS.md` lists them. They never depend on configuration or the environment. Candidates and
probabilities go to stderr or to the `--json` envelope, never to stdout where they could become
an argument.

Flags keep the letters every shell user knows: `-C DIR`, `-0`, `-q`, `--dry-run`, `--json`.

## What jevify is not

- It does not write commands, flags, messages or file names.
- It does not count, calculate or compare dates. Code does that before the model is asked.
- It is not a security gate. Text under judgment can argue with the judge.
- `route` is for the long tail of a large PATH. A frontier model already knows the common tools.

## Where an answer is worth having

jevify earns its call where checking the answer is cheaper than doing the work. A pointer to one
line, one branch or one file is checked at a glance. A subset is checked only by reading the
whole input, so `filter` always keeps the full output within reach.

The skill file is part of the product. An agent reaches for a tool only when it knows the
situations that call for it: a value it can describe and cannot spell, an option that depends on
text it has not read, an output too long to read, many records and one question. The skill also
teaches the one quoting habit: the whole marker argument in single quotes.

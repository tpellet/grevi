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

## Time is the cost

A judgment costs almost nothing. The keyless backend is free. What an agent pays for is time: a
model turn takes seconds, and a process that waits on the network holds that turn open. The
design follows from that.

- **One process per question, however many records.** A verb reads the whole stream, asks in
  parallel, and answers once. A shell loop that starts jevify once per file is the slow form;
  `filter --files` is the fast one.
- **At most two rounds of requests per verb.** Inside a round every request is in flight at the
  same time.
- **Output flows.** A per-record verb prints a record as soon as every record before it has an
  answer, so `| head` ends the work early.
- **Cheap tools go first.** `grep`, `awk`, `jq` and `head` narrow a stream at no cost. jevify
  judges what is left, and judges identical records once.
- **Limits protect time, not money.** One ceiling holds in every verb: 20,000 records. A
  selection also has to fit two rounds, which is 9,801 candidates without a key. A verb states
  its record and request counts before the first request and then does the work. Below those
  limits it never refuses a list because the list is long.

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
jevify fill -- mkdir '@{dir:where the release scripts live}/archive'
jevify fill -- kubectl logs '@{pod:the payment worker}'
cargo test -- --list | sed 's/: test$//' | jevify fill -- cargo test '@{-:the retry backoff cap}'
gh pr list --json number,title | jevify fill --key number -- gh pr view '@{-:the Windows path fix}'
cargo build 2>&1 | jevify fill -- "$EDITOR" '@{-:the file that fails to compile}'
```

Three families of value can be listed.

| Family | Where the candidates come from | Marker |
|:---|:---|:---|
| things that exist | a lister per kind | `@{branch:…}`, `@{commit:…}`, `@{file:…}`, `@{pod:…}` |
| options the caller writes | the marker itself | `@{one:a\|b\|c:…}`, `@{flag:--draft:…}` |
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
- `@{flag:--name:question}` is a whole argument. A yes leaves `--name` and a no removes the
  argument. An unsure answer is an abstention: nothing runs, and the status line gives the
  probability, so the caller writes or drops the flag itself in one turn. Leaving a flag out is
  not always the safe direction (`--dry-run`, `--draft`), so doubt never chooses it.
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

A long list is searched in two rounds. Round one splits the list into windows of one request
each (200 candidates with a key, 99 without), asks every window at the same time, and keeps the
three best of each window by rank. Round two is one request over all of those finalists with
their richest evidence, so the winner and its rivals meet in one comparison and their order
means something. Probabilities from different requests are never compared. `fill` searches
every list whose finalists fit one request: about 3,200 candidates without a key and 13,000 with
one. Above that, a list with a recency order keeps its newest part and the status line says so;
any other list is refused with the two ways to narrow it: a path prefix, or a piped list.
`pick`, `pick --from KIND` and `why` print and run nothing, so they keep two finalists of each
window, then one, while the finalists still fit one request: `pick --from KIND` searches up to
W × W: 9,801 candidates without a key, 20,000 with one.

| Kind | Candidates | Evidence |
|:---|:---|:---|
| `-` | records on stdin, or `--candidates FILE` | the whole record; `--key` or `--field` names the handle inside it |
| `branch` | local and remote refs | name, last commit subject, age |
| `commit` | the log of the current branch | subject, body, changed paths |
| `file`, `dir` | tracked and untracked files that are not ignored, hidden ones included | path, first lines |
| `tool` | the commands on the PATH, for `route` and `pick --from tool` | name and one-line manual summary |
| `pr`, `issue`, `ci-run`, `stash`, `process`, `container`, `pod` | a recipe: the owning tool's listing | the whole line of the listing |

**A kind is a recipe.** Most kinds are the `-` form with a name: the command that lists, and
which field is the handle. The lister's own flags choose what each line says. jevify ships its
recipes as data, one JSON object per line, and reads more from `kinds.jsonl` in the user's
configuration directory:

```
{"kind":"pod","list":["kubectl","get","pods","--no-headers"],"field":1}
{"kind":"pr","list":["gh","pr","list","--json","number,title,state"],"key":"number"}
```

A new kind is one appended line, and a recipe is shared by copying it. Only kinds that need
logic are code: `branch` folds a remote ref into its local twin, and `file` walks and withholds
secrets. jevify reads no recipe from a repository, so a clone never adds a command that `fill`
runs. A list with no kind is a pipe into `'@{-:…}'`, shaped by `sed`, `cut` or `jq` first.
`capabilities` lists every kind with its command.

- A literal prefix narrows a list: `'src/cmd/@{file:stages hunks}'` looks only under `src/cmd/`.
- Candidates come from the directory jevify runs in; `cd` changes it. jevify never reads the
  scope out of the tool's own flags.
- `one` and `flag` judge a context: stdin, or `--context FILE`.
- stdin has one role per call. It is the candidates of `@{-:…}` or the context of `one` and
  `flag`, never both; the other one comes from `--candidates FILE` or `--context FILE`. A call
  that gives stdin two roles is a usage error, exit 2.
- Markers resolve at the same time against one snapshot. If one marker abstains, nothing runs.
- When two candidates are close, round two fetches richer evidence. If they stay close, `fill`
  abstains and names both.
- Code handles order, counts and dates before the model is asked. "The newest branch" is a sort.

**The run.** `fill` becomes the command, as `env` and `nice` do: it replaces itself with the
resolved argument list, with no shell and no process in between. `--dry-run` prints the command,
quoted so that bash, zsh and dash read it back exactly, and runs nothing. It is one line unless
an argument holds a newline, which stays inside its single quotes. What it prints is the command
that a run executes.

- The caller's own permission system decides what may run; jevify is not a permission system.
- The command owns its output, its signals, its terminal and its exit code. jevify writes to
  stderr before it, every line starts with `jevify fill:`, and the last one is `exec` with the
  command, or `not run:` with a reason. `-q` keeps only `not run:`.
- Exit 2 to 6 when nothing ran; otherwise the exit code is the command's own. Exit 3 names
  `no_match`, `ambiguous`, `unsure_flag` or `insufficient_evidence`.
- `fill` runs a command only on an answer from Jev. The free backend names the model that
  answered; when another model answers, nothing runs and the line names it.
- A marker that reads stdin takes all of it, and the command receives an empty stdin.
  `--candidates FILE` and `--context FILE` leave stdin to the command. With no such marker the
  command inherits stdin and the terminal.
- `--json` goes with `--dry-run`. In a run, stdout belongs to the command and jevify reports on
  stderr.
- `fill` is the only place where jevify starts a command.

The agent uses it because one call replaces list, read, choose and act, and the listing never
enters its context. It gains most where the listing is long and the names are opaque: commits,
branches, tests, CI runs; and where an option depends on text the agent has not read. It gains
nothing for a name the agent has already seen or a string that a literal search finds.

### The output side: `why`, `pick`, `filter`, `label`, `is`

The command ran. Its output is long, or it needs a judgment before the next step.

| The agent wants | Verb | Classical twin | Returns |
|:---|:---|:---|:---|
| the line that explains a failure | `why` | — | one line with context |
| one record out of many | `pick 'description'` | `fzf --filter` | one record |
| only the records that matter | `filter 'statement'` | `grep` | a subset, and the path of the full output |
| a tag on each record, to sort or triage them | `label a,b,c` | an `awk` key | each record with its label |
| a decision to branch on | `is 'statement'` | `test` | an exit code |

The verbs form a small algebra over one type, the stream of records. `pick` and `filter` return
records of their input, byte for byte, and `label` puts a tag and a tab in front of each, so
`cut`, `awk`, `sort`, `uniq` and another jevify verb read their output as they read the input.
`why` points and does not reduce: it prints the line with its number and its context.

```
gh issue list | jevify filter 'reports a crash' | jevify filter 'names Windows'    # and is a pipe
gh pr list --json number,title | jq -c '.[]' | jevify filter 'touches the installer' | jq -r .number   # jq in, jq out
gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c       # a histogram by meaning
fd -0 -e json | jevify filter -0 --files 'a test fixture'                          # files by content
until kubectl get pods | jevify is 'every pod is ready'; do sleep 5; done          # wait on a meaning
cargo test 2>&1 | jevify is 'every failure is a network timeout' && cargo test     # retry only a flaky run
```

- `why` is for a log of any length: it searches, and returns one line. `filter` is for records:
  it judges every record, and returns many.
- A verb takes the flags of its twin, with the twin's meaning. `filter` has `-v` (the records
  where the statement is false) and `-c` (the count). `pick -n 3` is the three best, as
  `head -n 3`.
- jevify does not do what another tool does. `jq -c '.[]'` makes JSON into lines and `jq -r`
  reads a field back. `cut` and `awk` take a column. `grep -n -C2 -F -f <(jevify filter 'x' < log) log`
  adds line numbers and context, because the records `filter` prints are fixed strings of its
  input. `tee`, `head` and `sort` work as they always do.
- A condition is written so that yes means act. `if`, `&&` and `until` treat exit 1, exit 3 and a
  network failure alike, as "do not act".
- Output verbs read stdin and never start a command. They send the evidence to the configured
  backend and may save the input on this machine; they do nothing else.
- A record is a line. `--para` makes it a block between blank lines, for test failures and stack
  traces. `-0` reads NUL-separated records. `--files` reads paths and judges each file's first
  lines. `why` takes none of the three. Records come out unchanged and in their input order.
  Without a key each record is judged alone, up to sixty in one request, so ten thousand
  lines are about 170 requests. With a key twenty records share one request, and each question
  names its record.
- `pick --from KIND 'description'` selects among a kind's candidates in place of stdin and prints
  the handle. `route 'task'` is `pick --from tool`: it names the installed tool for a task, with
  its summary and synopsis, and runs nothing. The agent writes the command.
- A printed handle is for reading. As an argument it goes through `fill`: in
  `git switch "$(jevify pick …)"` the shell drops jevify's exit code, and an abstention becomes an
  empty argument that the command accepts.
- A reduction always leaves a way back: the full input is saved and its path is printed.
  `filter` keeps what it is unsure about and says so: `kept 31 of 10074, 2 unsure, full output:
  PATH`. `--strict` drops the unsure ones. The two errors differ in cost: an extra record costs a
  glance, a dropped record costs the whole input. `pick` and `fill` face the opposite costs and
  abstain. A reduction that could not save its input says so and never claims to be complete.
- `is 'a' 'b' 'c'` asks several statements in one request and prints one verdict per line. With
  one statement it prints nothing, as `test` does. It exits 0 when all hold, 1 when one does
  not, 3 otherwise. `is --context FILE` judges a file in
  place of stdin, so `is` is a predicate for `find -exec` and for a `make` rule.
- `jevify fill -q -- CMD 2>&1 | jevify why` reads the command's output alone. When nothing ran,
  the one `not run:` line is the true cause, and `why` may point at it. A script that must stop
  on the first stage sets `pipefail`.

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

Flags are few and keep the letters every shell user knows: `-v` and `-c` as on `grep`, `-n N` as
on `head`, `-0` as on `xargs`, `-q`, `--dry-run`, `--json`. What another tool does well has no
flag in jevify.

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

The README teaches the same language to a person, in the same order: the two sides, the verbs
with their twins, the marker, the exit codes. `jevify init agents` prints the block that teaches
it to every agent that reads an `AGENTS.md` file, so the language reaches an agent with no
plugin system.

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
   jevify fill -- git switch @branch:'…'      cargo test 2>&1 | jevify why
   a description becomes a real argument      a wall of output becomes an answer
```

### The input side: `fill`

The agent knows the command and the operation. It does not know the handle: which branch, which
file, which commit, which pod, which process. Without jevify it lists, reads the listing, chooses,
and then acts. That is two or three turns, and the whole listing enters its context.

```
jevify fill -- git switch @branch:'the auth refactor'
jevify fill -- mv @file:'the March invoice' @dir:'tax documents for this year'
jevify fill -- mkdir @dir:'where the release scripts live'/archive
gh pr list --json number,title | jevify fill --key number -- gh pr view @-:'the Windows path fix'
```

- A marker is one whole argument: `@type:'description'`. The type names a kind of thing, not a
  command. About thirty kinds cover most tools: file, dir, branch, commit, tag, stash, pr, issue,
  ci-run, container, image, pod, process, target, script, test, host.
- `@-:'description'` takes the candidates from stdin, so any tool that can print a list works
  without any built-in knowledge.
- A tool's own completion protocol is one more source of candidates where it exists.
- Everything that is not a marker passes through byte for byte. jevify never parses the tool's
  flags and keeps no table of commands.
- `fill` prints the resolved command and the evidence for its choice. `--run` executes it. The
  caller's own permission system decides what may run; jevify is not a permission system.
- New values stay literal. `fill` resolves references to things that exist: the parent directory
  of a new folder, the source of a copy, the branch to rebase onto.

The agent uses it because one call replaces list, read, choose, and the listing never enters its
context.

### The output side: `why`, `pick`, `filter`, `label`, `is`

The command ran. Its output is long, or it needs a judgment before the next step.

| The agent wants | Verb | Returns |
|:---|:---|:---|
| the line that explains a failure | `why` | one line with context |
| one record out of many | `pick 'description'` | one record |
| only the records that matter | `filter 'statement'` | a subset, and the path of the full output |
| a tag on each record, to route or triage | `label a,b,c` | each record with its label |
| a decision to branch on | `is 'statement'` | an exit code |

- Output verbs read stdin and never start a process, so they are safe to allow without review.
- A reduction always leaves a way back: the full output is saved and its path is printed.
- Several questions about one input share one request.

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

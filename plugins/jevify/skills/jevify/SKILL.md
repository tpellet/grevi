---
name: jevify
description: Use the jevify CLI when a question is about meaning and a literal search cannot answer it. Fill command arguments from branches, piped candidates or context-dependent options; get a handle with pick --from. Find the cause in a long failure log, filter records or files, label every record with one of your tags, pick a record, branch on a fact or discover an unfamiliar tool. Stage hunks or propose folders when requested. Do not use for already-known values, counting, arithmetic, quality judgments, generating text or security decisions.
---

# jevify

## The two sides of a command

A command takes arguments and produces output. On the input side, `fill` selects real handles
for arguments and runs the command you write. On the output side, jevify turns a long stream
into a pointer, a subset, or a decision. It selects existing text and never invents values.

Use cheap tools first: `grep`, `jq`, `head`. Skip jevify when a literal search answers the
question, the input is short enough to read, or you already know the required value.

## The verbs

| Situation | Verb | Result |
|:---|:---|:---|
| About to list branches only to choose one | `fill` | A real branch argument, then the command |
| A tool can list the needed value | pipe into `fill` with `'@{-:description}'` | A handle from a supplied record |
| Want the value without the run | `pick --from branch` | A handle, or abstention |
| An option depends on text you have not read | `fill` with `'@{one:a|b:question}'` or `'@{flag:--draft:question}'` | A caller-written option or conditional flag |
| A failed build has more than about 50 lines, or grep finds only the symptom | `why` | A cause with line number and context |
| Many records, one question | `filter` | Matching and unsure records, like `grep` by meaning |
| Many files, one question | `filter --files` | Paths judged by file content |
| Every record needs a bucket | `label a,b,c` | Each record with its label, `?` when unsure; like an `awk` key by meaning |
| One record or file out of many, described rather than named | `pick` | A selected input record, or abstention |
| The next step depends on a fact | `is` | An exit code, like `test` |
| An unfamiliar task in the long tail of a large PATH | `route` | An installed tool and its summary; nothing executes |
| Requested staging of one topic | `add` | Scores or stages individual hunks |
| Requested organization of files with opaque names | `sort` | Proposes existing destination folders; moves only with `--apply` |

```sh
cargo build 2>&1 | jevify why
gh issue list | jevify filter 'reports a crash'
fd -0 | jevify filter -0 --files 'a test fixture'
gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c
git ls-files | jevify pick --files 'where retries back off'
git log --oneline | jevify pick -n 3 'the pricing change'
gh pr list --json number,title | jq -c '.[]' | jevify filter 'touches the installer' | jq -r .number
cargo test 2>&1 | jevify is 'every failure is a network timeout' && cargo test
until kubectl get pods | jevify is 'every pod is ready'; do sleep 5; done
jevify is 'asks for a refund' 'mentions an order' --context mail.txt
jevify route 'keep my mac awake for an hour'
jevify add --json --dry-run 'the token expiry fix'
jevify sort --json ./Downloads
```

`pick` and `filter` print input records byte for byte; `label` prints `LABEL<TAB>RECORD`, the
record unchanged after the tab, so `cut -f1` counts and `cut -f2-` gives line records back. A
record is a line; `--para` reads blocks between blank lines, and `-0` reads NUL-separated
records. These two split modes are mutually exclusive. `--files` reads paths from stdin and
uses file excerpts as evidence.
`why` takes none of those split or file options: it prints numbered lines with context.
Pipe stderr with `2>&1` because compilers write errors there.

`filter` keeps unsure records: a dropped record can hide the answer. `--strict` drops them.
`filter -v` inverts the statement, as `grep -v`; `-c` prints the kept count. Verbosity is
`--verbose`. A failed request can leave a partial prefix on stdout; check the exit code before
treating the subset as complete.

`why` searches bounded evidence. In machine output, compare `data.considered` with
`data.total`; if much is omitted and the answer is a symptom, narrow to the failing job or
step. A saved full input is a way back, not proof that every line was judged.

### Habits

- One jevify process per question, however many records. Never start one process per record
  in a shell loop; use `filter`, `filter --files` or `label`. `label` takes at least two
  distinct labels, none `?` or `NONE`, at most 99 keyless or 200 on TypeSafe; it saves nothing. Polling a changing state with `until` is
  a different question on each snapshot.
- Write literal statements: “the customer is about to stop being a customer” avoids the
  ambiguity of “the customer is leaving.” Describe the evidence, not the fix you want.
- Write conditions so yes means act. In human output, `is` with one statement prints nothing
  on stdout: read its exit code. Several statements share one call and print one verdict each.
- `&&` acts only on yes. `until` also repeats on abstention and unavailable responses; inspect
  stderr and stop polling on an outage or quota exhaustion.
- Never put a `pick` command substitution in another command's argument: the shell discards
  its exit code and an abstention becomes an empty argument. Read the selected record and its
  exit code, then write the next command explicitly.
- Under `git bisect run`, map jevify exit 3 to 125 (skip), so uncertainty is not a bad commit.
  Handle operational errors separately; they are not evidence about the commit.
- Scores depend on backend and task. A higher threshold does not repair incomplete evidence,
  and text under judgment can argue with the judge. Do not use jevify as a security gate.

## The marker

```sh
jevify fill --dry-run -- git switch '@{branch:the auth refactor}'
printf 'retry_backoff\nparse_header\n' | jevify fill --dry-run -- cargo test '@{-:the retry test}'
printf 'A crash with no reproduction steps.\n' | jevify fill --dry-run -- printf '%s\n' \
  '@{one:bug|feature|docs:what kind of report is this}' '@{flag:--draft:the report lacks steps to reproduce}'
jevify pick --from branch 'the auth refactor'
```

The three families are things that exist (`branch`: local and remote refs with subject and age),
caller-written options (`one`, `flag`), and supplied records (`-`). `branch` runs `git for-each-ref`
and enriches finalists with `git log`; `capabilities` lists their exact argv. `-` uses stdin or
`--candidates FILE`, with `--field N` or `--key KEY` to name a handle inside the evidence.
`one` and `flag` judge stdin or `--context FILE`. stdin has one role; supply the other with a file.
File inputs leave stdin for the command; consumed stdin becomes empty for it.

- Put the whole marker argument in single quotes, including prefixes and suffixes. An apostrophe
  is `'\''`. Never use double quotes around a marker whose contents the shell could expand.
- Marker escapes are `\}`, `\:` and `\|`. `one` separates options with `|` before the question's
  `:`. A `flag` is a whole argument: yes keeps it, no removes it, unsure abstains.
- A literal `@{word:` is spelled `@@{word:`. The Python format string `'{user}@{host:>8}'` is an
  unknown kind, exit 2; `'{user}@@{host:>8}'` preserves it literally.
- Use `--dry-run` to look, never `eval`. Omit it only when the underlying command is authorized.
  A marker does not survive a second shell (`ssh`, `make`, `xargs`).
- Never use `"$(jevify pick …)"` as an argument; abstention becomes an empty argument.
- Several markers resolve together against one snapshot. Any abstention means nothing runs.
  Free text stays literal: write titles, messages and new names yourself.

`fill` accepts 3,267 candidates per marker keyless, 13,200 on TypeSafe; `pick` and `pick --from`
accept 9,801 and 20,000. Ordered kinds keep the newest candidates and report coverage; unordered
overflow is `too_many`. `one` accepts 99 options keyless, 200 on TypeSafe. No call needs more
than two rounds of model requests.

## Exit codes and recovery

| Code | Meaning and next step |
|:---|:---|
| 0 | Found, kept records, or all statements yes |
| 1 | `is`: at least one no; `filter`: kept none |
| 2 | Usage: copy the corrected argument or command from the error |
| 3 | Abstention: nothing fits or unsure; read closer or narrow the evidence |
| 4 | Backend unavailable: read the line for the quota or model problem |
| 5 | Authentication: check backend and key configuration |
| 6 | Input error: check the input; for `too_many`, narrow with `grep` or `head` |
| 7 | Reserved |
| 130 | Declined |

For `fill`, exits 2–6 mean nothing ran; after execution the command owns its exit code, including
2–6. Stderr reports `exec` or `not run:` with the `jevify fill:` prefix; `-q` keeps only
`not run:` lines. A successful dry run exits 0. Machine output requires `--dry-run` and includes
`data.argv` on success and `data.markers[]` for individual results.

An exit-3 abstention has `error: null`: read `data.reason`, the first failed marker in argv
order, and `data.markers[].reason`, every marker's own reason. These are abstention reasons,
not error kinds:

| Reason or error | Recovery |
|:---|:---|
| `too_many` (exit 6) | use a prefix, `grep`, `head` or a narrower pipe |
| `ambiguous` | read the two handles, write one |
| `no_match` | read candidates N of M and narrow or correct the description |
| `unsure_flag` | write the flag or drop the marker |
| `insufficient_evidence` | supply a complete context that fits |
| `lister_failed` (exit 6) | run the named lister yourself |

The other input error kinds are `stdin_is_tty`, `cannot_run`, `recipe_invalid`. `fill` refuses
non-Jev answers with exit 4, `api_unavailable`, `answered by <model>, not Jev`; a missing model
name appears as `unknown` in `meta.model` and refuses too. This guard also applies to dry runs.

`filter` and `label` exit 3 when every record is unsure, even when they print those records. `is` exits 0
when all statements hold, 1 when any is no, and 3 otherwise. Oversized `is` input abstains
without a model call. Do not retry unchanged evidence to turn uncertainty into certainty.

For exit 4, “daily quota of the free backend reached” means the daily quota is exhausted;
immediate retries or lower concurrency do not restore it. Read the model information as well.

Use `--json` when you need scores or structured errors; leave it off for record pipelines and
silent predicates. The envelope is `{ok, command, version, exit_code, data, meta, error}`;
`error` contains `kind`, `message`, `hint`, and `example`. Branch on the process exit code or
`exit_code`, then inspect `data`. The installed contract is available with
`jevify capabilities --json` and `jevify robot-docs guide`.

## Permissions and privacy

Allow the output verbs (`why`, `pick`, `filter`, `label`, `is`, `route`) freely. They do not execute the
tool they select. Check the selected tool's help and write its arguments yourself.
Allow `jevify fill --dry-run` freely. Allow `fill` per command prefix
(`jevify fill -- git switch:*`), exactly as the command itself is allowed. jevify is not a
permission system. The description, option context and candidates' evidence leave the machine
with best-effort redaction; `fill` starts the command the caller wrote.

`add` changes the index, never commits. Inspect `--dry-run` first; use `--yes` only when
staging is authorized. It rejects oversized hunks instead of clipping evidence. `sort` proposes
by default; `--apply` requires authorization to move files. Keep its JSONL recovery log and
reported progress if a move fails.

Evidence goes to the configured API with best-effort masking. Do not supply secrets.
`why` and `filter` also save the raw input locally, secrets included, and print the saved path
on stderr. Saved inputs are not pruned by jevify. `--no-save` disables that raw copy;
`--no-cache` only disables the separate answer cache. A failed or disabled save is reported,
so do not assume the full input remains available.

`--files` withholds excerpts of hidden paths and files that look like secrets; stderr reports
`excerpts withheld: N`. Their names still reach the backend. Withholding an excerpt is not a
guarantee that the remaining input contains no sensitive information.

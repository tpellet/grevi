---
name: jevify
description: Use the jevify CLI when a question is about meaning and a literal search cannot answer it. Find the cause in a long failure log, filter many records or files with one question, pick one record by description, branch on a fact, or discover an unfamiliar installed tool. Stage hunks by topic or propose folders for files when requested. Do not use for short input, known commands, counting, arithmetic, quality judgments, generating text, or security decisions on untrusted input.
---

# jevify

## The two sides of a command

A command takes arguments and produces output. On the output side, jevify turns a long stream
into a pointer, a subset, or a decision. It selects existing text and never generates an
argument or a command. Write the command yourself; let jevify judge the evidence it produces.

Use cheap tools first: `grep`, `jq`, `head`. Skip jevify when a literal search answers the
question, the input is short enough to read, or you already know the exact tool.

## The verbs

| Situation | Verb | Result |
|:---|:---|:---|
| A failed build has more than about 50 lines, or grep finds only the symptom | `why` | A cause with line number and context |
| Many records, one question | `filter` | Matching and unsure records, like `grep` by meaning |
| Many files, one question | `filter --files` | Paths judged by file content |
| One record or file out of many, described rather than named | `pick` | A selected input record, or abstention |
| The next step depends on a fact | `is` | An exit code, like `test` |
| An unfamiliar task in the long tail of a large PATH | `route` | An installed tool and its summary; nothing executes |
| Requested staging of one topic | `add` | Scores or stages individual hunks |
| Requested organization of files with opaque names | `sort` | Proposes existing destination folders; moves only with `--apply` |

```sh
cargo build 2>&1 | jevify why
gh issue list | jevify filter 'reports a crash'
fd -0 | jevify filter -0 --files 'a test fixture'
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

`pick` and `filter` print input records byte for byte. A record is a line; `--para` reads
blocks between blank lines, and `-0` reads NUL-separated records. These two split modes are
mutually exclusive. `--files` reads paths from stdin and uses file excerpts as evidence.
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
  in a shell loop; use `filter` or `filter --files`. Polling a changing state with `until` is
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

`filter` exits 3 when every record is unsure, even when it prints those records. `is` exits 0
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

Allow the output verbs (`why`, `pick`, `filter`, `is`, `route`) freely. They do not execute the
tool they select. Check the selected tool's help and write its arguments yourself.

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

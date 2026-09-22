# Paired agent pilot on the seven verbs

Bead `hunch-evl`. Every number below was measured on 2026-09-22 with jevify 0.8.3 (the installed
release binary, commit `38b8b5c`), answering model `jev-1.13.0`, on a macOS dev machine. The
backend is TypeSafe with a key unless a row says classifier.dev (keyless). The agent model is
`claude-haiku` in both arms.

This is a pilot, not a confirmation. It does not support a claim that jevify makes an agent more
accurate: both arms answer the same tasks, and the arm without jevify answers 8 of 8.

## The design

Eight tasks, each run twice: once by an agent whose instructions are the output of
`jevify init agents` and a path to the binary, once by an agent told that jevify is not available.
Nothing else differs. The tasks: a branch by description, a commit by description, a failed CI run
on a tag, the root cause in a 300-line Go CI log, a source file by content, tests by topic, triage
of 12 issue reports, the tool that renders a terminal demo.

Every jevify call went through a logging wrapper that records verb, flags, exit code, request count
and latency. The counts below are read from that log, not from the agents' own reports: an agent
that says it ran a command is not evidence that it did.

**Isolation.** The two arms run as separate processes with separate instructions, separate binary
paths and no shared writable state; neither arm produces an artifact the other could read. That is
isolation by construction, not isolation enforced by an OS sandbox: both agents run as the same
user on the same filesystem, and one of them (T6, without jevify) wrote two scratch files under
`/tmp` although the instructions said read-only. The bead's first Done-when item, isolation enforced
by the OS or the harness, is **not met**; it is enforced by construction and by inspection of the
transcripts afterwards.

## Adoption, correctness, resources

jevify was called in **4 of 8** with-arm runs (T1 `pick --from branch`, T3 `fill --dry-run` and
`why`, T4 `why`, T7 `label --files` twice) and in **0 of 8** without-arm runs. Six calls in all,
every one exit 0; no abstention, no misquoted marker, no usage error. Not used where a literal
search was enough: T2 (`git log --grep` found the commit), T5 (`grep` found the SSRF module),
T6 (`grep` over test names), T8 (`which vhs`).

Correctness: **8 of 8 without jevify, 7 of 8 with**. The difference is T3, where both arms name the
right failing test and the with-arm quotes the line `why` pointed at — the panic header at line 185
(p 0.39) — instead of line 189, the `no_match; candidates 0 of 0` line that explains the failure.
The without-arm quotes line 189. On T6 both arms list the same 12 test functions, the 7 in the gold
plus 5 neighbours.

| Task | turns with / without | tool uses | input tokens | output tokens | wall time s | jevify calls |
|:---|---:|---:|---:|---:|---:|:---|
| T1 branch by description | 11 / 53 | 4 / 25 | 313k / 1,712k | 1,564 / 5,573 | 19 / 64 | 1 `pick`, exit 0 |
| T2 commit by description | 17 / 13 | 6 / 5 | 495k / 370k | 1,740 / 1,427 | 19 / 15 | 0 |
| T3 failed CI run on a tag | 19 / 24 | 8 / 10 | 594k / 772k | 2,913 / 2,306 | 47 / 37 | 2 `fill`, `why`, exit 0 |
| T4 root cause in a CI log | 14 / 9 | 4 / 3 | 532k / 326k | 2,600 / 1,449 | 32 / 19 | 1 `why`, exit 0 |
| T5 source file by content | 14 / 24 | 5 / 10 | 522k / 817k | 2,143 / 2,904 | 23 / 31 | 0 |
| T6 tests by topic | 47 / 66 | 22 / 31 | 2,478k / 2,645k | 5,810 / 9,763 | 73 / 108 | 0 |
| T7 triage of 12 reports | 10 / 32 | 3 / 14 | 288k / 955k | 1,977 / 4,854 | 20 / 46 | 2 `label`, exit 0 |
| T8 the tool for a job | 12 / 12 | 6 / 6 | 333k / 321k | 1,366 / 1,422 | 15 / 15 | 0 |
| **total** | **144 / 233** | **58 / 104** | **5.56M / 7.92M** | **20.1k / 29.7k** | **248 / 335** | **6** |

Cost per task pair: NOT RUN. The token counts above are the harness's own per-run usage and no
price was applied to them; the invocation log records requests and latency, not the envelope's cost
estimate.

The with arm spends 62 % of the turns and 70 % of the input tokens of the without arm. Three pairs
carry the difference (T1, T7, T5), and in T1 and T7 the with arm made the jevify call that replaced
the listing-and-reading loop: T1 without jevify walked twelve branches one `git log` at a time
(25 tool uses), T7 without jevify read all twelve reports (14 tool uses). The three pairs where the
with arm cost more (T2, T4, T8) are pairs where it did not call jevify, or called it and then read
the input anyway. Eight pairs, one run each: the spread between pairs is larger than the difference
between arms, so these are figures, not an effect.

Two of the six calls were answered from the local answer cache (`requests: 0`): the `why` calls on
T3 and T4 repeat inputs an earlier round measured. The latency and backend cost of the with arm are
therefore lower than a first run would be; turns, tool uses and token counts are unaffected.

## The four items folded in from `hunch-lpp`

### `filter` at the unsure boundary, labelled independently

Measured on the frozen validation set (`evals/validation`, 153 cases, two independent annotators
with adjudication, gold held apart from the manifest), 40 `filter` cases, both backends, from the
run recorded in `benchmarks/results.md`.

| Gold | n | TypeSafe decision | classifier.dev decision |
|:---|---:|:---|:---|
| keep | 17 | 17 keep | 17 keep |
| drop | 19 | 19 drop | 19 drop |
| unsure | 4 | 4 drop | 4 drop |

Recall on the records both annotators agreed to keep is 17/17 on both backends, and no record they
agreed to drop is kept. The band never fires: the four records whose adjudicated gold is `unsure`
(`Merge branch 'pr-NNN'` under "the change is a bug fix" — the subject says nothing either way)
score 0.13–0.17 on TypeSafe and 0.00–0.01 keyless, far below the 0.35–0.65 band, so the verb reads
"the record does not say" as a confident no. The lowest keep-gold score is 0.77 (TypeSafe) and 0.79
(keyless), the highest drop-gold score 0.28 and 0.20: the decided records separate cleanly and the
unsure ones sit inside the no. No threshold moves them.

### Finalist miss with a real hook

**No hook exists.** jevify 0.8.3 exposes neither the round-one ranking nor the finalist set: the
`Shortlist` returned by `tournament::shortlist` stays inside the process, `--json` prints only
candidates that beat NONE in the deciding round, and no environment variable turns round one on.
The figure below is a reconstruction, not a reading of the real run: each list is cut into the same
`W = 200` windows the code uses, the window holding the gold is ranked on its own with
`pick -t 0 -n 200`, and the gold's rank in that window is read from the answer. Round-one windows
are independent, so the reconstruction is exact up to run-to-run variation and up to candidates
scoring at or below NONE, which are not printed.

36 cases over three lists above one window (`testify` commits 409 lines / 3 windows, pydantic-ai
paths 2,834 / 15, jevify test names 209 / 2), 22 straightforward descriptions and 14 written to be
hard:

- the gold ranked **first in its window in 36 of 36 cases**, so the finalist miss rate is **0/36 at
  n = 3, n = 2 and n = 1**.
- end to end the two-round run answered **30 of 36** correctly. All six losses happen in round
  two, against a sibling the round has put next to the gold: `docs/examples/weather-agent.md` for
  `examples/.../weather_agent.py`, `docs/examples/rag.md` for `rag.py`,
  `docs/capabilities/web-fetch.md` for `capabilities/web_fetch.py`, `_tool_search.py` for
  `capabilities/_tool_search.py`, a test cassette for `temporal/_event_stream.py`, and a
  neighbouring commit for `d3d5264`.

Narrowing the window does not lose the answer; the finals throw it away. The same shape was seen
at 0.7.0 (0/36 and 30/36).

### Jev as a judge over these transcripts

Four statements, verbatim, run as one `jevify is --json --context <transcript>` call per transcript
over all 16:

1. `the agent called the jevify tool at least once`
2. `the agent read a long command output in full where a filter or a root-cause search could have reduced it`
3. `the agent listed many candidates and chose one of them by reading the whole listing`
4. `some command the agent ran exited with a non-zero status`

Statement 1 has a ground truth outside the transcript, the invocation log, so it is the one that can
be scored: **TypeSafe 15 of 16 right and 1 unsure** (T4 without jevify, 0.45; the file paths in that
transcript contain the word `jevify`), **classifier.dev 15 of 15 answered right** — the sixteenth
transcript (39.7 KB) exceeds the keyless evidence budget and is refused with exit 3, `truncated`,
0 requests, which is the honest answer and costs no quota.

Statement 4 agrees with the agents' own reported exit codes on 14 of 16; both disagreements are a
confident `yes` (0.93, 0.84) where the agent reported every command as exit 0. The agents
under-report the exit codes of pipeline members, so that ground truth is not reliable enough to
call either side wrong. Statements 2 and 3 have no ground truth that does not beg the question:
answers on them are recorded, not scored.

`jevify label` over the 162 commands the 16 agents ran, into
`list-candidates,literal-search,read-whole-file-or-output,inspect-one-item,jevify`: TypeSafe
labelled 125 and left 37 unsure, found **6 of the 6 real jevify invocations** and gave one false
`jevify` at 0.75 (`cd ~/Projects/testify && pwd`). The keyless backend over the first 60 of those
commands: one request, 53 labelled, 7 unsure, **4 of the 4 real invocations, no false positive**.

### The keyless backend

The whole frozen validation set is already measured on classifier.dev without a key; the per-verb
coverage, accuracy, abstention and false-action figures, the request and question counts and the
nine cases where the two backends decide differently are in `benchmarks/results.md`. This pilot
adds the judge runs above: 15 `is` calls of four statements and one `label` call of 60 records,
about 120 classifications of the 20,000 a day one IP is allowed.

## What these numbers cannot support

- Both arms answer 8 of 8 (the with arm 7 of 8 on the quoted line of T3). Adoption is not
  effectiveness: nothing here shows jevify changed an outcome for the better.
- One run per cell, eight pairs. The turn and token gaps are dominated by three pairs.
- The isolation between arms is by construction, not enforced.
- Two of the six calls were cache hits, so the backend cost of the with arm is understated.
- The finalist-miss figure is a reconstruction of round one, because no hook exposes it.

## Beads filed

`hunch-93l` (P2, `pick` prefers a documentation page to the source file a description names),
`hunch-8b3` (P3, no hook exposes round-one ranks or the finalist set), `hunch-n35` (P3, `why`
points at the panic header rather than the line that explains the failure), `hunch-63r` (P3,
the unsure band never fires: `filter` reads "the record does not say" as a confident no).

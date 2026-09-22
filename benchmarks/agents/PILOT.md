# Paired agent pilot on the seven verbs

Two runs of the same eight tasks. The **first run** (bead `hunch-evl`) was measured on 2026-09-22
with jevify 0.8.3 (the installed release binary, commit `38b8b5c`); its isolation held by
construction only and its cost was measured per arm. The **second run** (bead `hunch-iu8`, later
the same day, jevify 0.9.1, commit `4279c06`) repeats the eight tasks under enforced isolation
and records cost per pair; it is in [its own section](#second-run-enforced-isolation-and-cost-per-pair)
and both sets of figures stay side by side. Answering model `jev-1.13.0`, macOS dev machine,
TypeSafe with a key unless a row says classifier.dev (keyless), agent model `claude-haiku` in
both arms, one run per cell.

The two runs are not comparable case by case: between 0.8.3 and 0.9.1 `filter` gained an unsure
band (a record that says nothing either way is kept, not dropped), `why` points at the line that
carries a panic's message instead of the `panicked at` header, and `pick` over a plain listing
prefers the entry that is the thing described over a page that documents it (`CHANGELOG.md`,
0.9.0). The second run also confines every command to a wrapper, forbids the Read, Grep and
Glob tools, and gives each run a fresh answer cache; the first run did none of these.

This is a pilot, not a confirmation. It does not support a claim that jevify makes an agent more
accurate: both arms answer the same tasks, and the arm without jevify answers 8 of 8 in both runs.

## First run (0.8.3): isolation by construction, cost per arm

### The design

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

### Adoption, correctness, resources

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

## Second run: enforced isolation and cost per pair

Bead `hunch-iu8`, 2026-09-22, jevify 0.9.1 (commit `4279c06`), `jev-1.13.0`, TypeSafe with a
key, `claude-haiku` in both arms, one run per cell. Same eight tasks, same gold, same
`jevify init agents` block (0.9.1's, 3,584 characters) as the only documentation the with arm
gets. Raw material outside the repository under `eval/r4/`: sixteen transcripts, the per-run
jevify log, the sixteen scan outputs and the canary record. The runner is
`scripts/pilot_r4/` (`setup.py` builds the run directories, `jevify_log.py` wraps the binary,
`check.sh` marks and scans, `collect.py` reads the transcripts, `canary.sh` is the canary run).

### How isolation was enforced

Each of the sixteen runs has its own directory `runs/T<n><arm>/` with `work/` (the working
directory), `cache/` (a fresh `JEVIFY_CACHE_DIR`, so no run can hit another run's answers),
`tmp/`, and `bin/run`, a wrapper that executes one quoted shell command under
`sandbox-exec` (macOS Seatbelt) with a profile generated per run:

- **writes**: denied everywhere except the run directory and `/dev`;
- **reads**: `file-read-data` denied on the whole evaluation tree except the task inputs
  (`sets/`) and the run itself, so no run can read the gold, another run, or the first run's
  transcripts;
- **the control arm** additionally cannot read or execute `~/.cargo/bin/jevify`, any
  `target/*/jevify`, or `~/.ssh/typesafe-ai-key`.

The agent is told to use only the Bash tool and to run every command through the wrapper. Of the
118 Bash calls in the sixteen runs, 117 went through it; the one that did not was
`echo "Test completed successfully"` (T3 without). No run used Read, Grep, Glob, Write or Edit.
The with arm's `jevify` is a logging wrapper inside the run directory that sets the run's cache
and key file (the key is read by jevify, never by the harness) and appends one JSON line per call.

**Canaries** (the containment checks of the 2026-09-20 protocol, `EXPERIMENT.md` stage 1), run
through both arms' wrappers before the tasks:

| Canary | with arm | without arm |
|:---|:---|:---|
| write into `~/Projects/testify` | denied | denied |
| write into `/tmp` | denied | denied |
| read the sentinel `runs/_sentinel/gold.txt` | denied | denied |
| list another run's directory | denied | denied |
| the key file readable (`test -r`) | **readable** | denied |
| execute `~/.cargo/bin/jevify` | allowed (the arm needs it) | denied, exit 126 |
| execute `target/release/jevify` | allowed | denied, exit 126 |
| write inside `work/` | allowed | allowed |
| `mktemp` in the system temp dir | denied (mktemp ignores `TMPDIR`) | denied |
| `gh run list` over the network | allowed | allowed |
| a write from Bash outside the wrapper | **not stopped**, named by the scan | same |

Two limits are real: the key must stay readable to the with arm's shell because jevify reads it
inside the same sandbox, so a with-arm agent could `cat` it (none did; the transcripts contain no
read of `~/.ssh`); and a command the agent runs without the wrapper is outside the sandbox, which
only the post-run scan catches. The control arm's binary rule matched directories named `jevify`
under `target/` too, so one `find` in T6 without skipped ten of them ("Operation not permitted");
it found the answer anyway.

**The check after each run.** `check.sh mark` drops a time marker before the run;
`check.sh scan` lists every file or directory newer than the marker under `~/Projects`,
`~/.config`, `~/.cache`, `~/.cargo`, `~/.local`, `~/.ssh`, `/private/tmp`, the user temp
directory and the evaluation tree, outside the run's own directory. Sixteen scans, **nothing
written by any arm outside its directory**. Everything the scans listed belongs to the host or
the harness: the Agent Mail server's log and sqlite, the Claude Code statusline cache, a `bunx`
cache, the harness's task output files, a launchd backup log, macOS daemon temp items, and (batch
4) files this evaluator wrote under `r4/` while bisecting the T7 defect after the runs. The first
run's "T6 without wrote two files under `/tmp`" cannot recur: the write is denied.

### Adoption, correctness, cost per pair

jevify was called in **5 of 8** with-arm runs (T1 `fill` twice, T3 `fill` and `why --json`, T4
`why --json`, T7 `label` three times, T8 `route`), nine calls, and in **0 of 8** without-arm runs.
Exits: 6 of 9 exit 0; T1's first `fill` exit 6 (`lister_failed`: the run's working directory is
not a git repository, the agent then `cd`'d), T1's second `fill` exit 128 (see `hunch-x4w`),
T7's first `label` exit 3 (paths without `--files`: 12 unsure). Not used on T2 (`git log --grep`),
T5 (`find` and `cat`), T6 (`grep`).

Correctness: **8 of 8 without, 7 of 8 with**. T3 with quotes the right line this time (the
`no_match; candidates 0 of 0` line, 189, that 0.8.3's `why` missed). T4 without quotes lines 238
and 239 correctly and misnumbers `WARNING: DATA RACE` as 141 (it is 155); counted correct. T6:
both arms list the same 8 functions, the 7 in the gold plus one neighbour. The with-arm miss is
**T7**: `label --files bug,feature,docs` over the twelve reports returned `docs` for all twelve,
exit 0, "labelled 12 of 12, 0 unsure", and the agent reported 0 / 0 / 12 against a gold of
5 / 4 / 3 (which the without arm got by reading the twelve files). The cause is the sandbox meeting
a silent fallback in jevify: the excerpt reader walks every path component from `/` and the
profile denies reading the evaluation directory above `sets/`, so every excerpt failed and each
report was labelled from its file name alone at p 0.77–0.99 with `excerpts_withheld: 0`
(`hunch-3fj`). Outside the sandbox the same command gives 5 / 4 / 2 with one unsure. `cat`, `wc`
and Python read the same files under the same profile.

| Task | turns with / without | tool uses | input tokens | output tokens | wall s | jevify calls (with) | requests | questions | cache hits | cost USD |
|:---|---:|---:|---:|---:|---:|:---|---:|---:|---:|---:|
| T1 branch by description | 15 / 24 | 6 / 10 | 452k / 711k | 3,050 / 3,756 | 33 / 38 | 2 `fill` (exit 6, 128) | 1 | 2 | 0 | 0.00004 |
| T2 commit by description | 7 / 11 | 2 / 4 | 197k / 313k | 1,522 / 1,673 | 17 / 18 | 0 | 0 | 0 | 0 | 0 |
| T3 failed CI run on a tag | 17 / 21 | 7 / 8 | 541k / 705k | 3,656 / 3,380 | 53 / 42 | `fill`, `why --json` | 4 | 8 | 0 | 0.00155 |
| T4 root cause in a CI log | 5 / 13 | 1 / 4 | 151k / 495k | 970 / 2,922 | 13 / 32 | `why --json` | 3 | 6 | 0 | 0.00073 |
| T5 source file by content | 25 / 25 | 11 / 11 | 892k / 790k | 3,999 / 3,335 | 58 / 53 | 0 | 0 | 0 | 0 | 0 |
| T6 tests by topic | 37 / 92 | 17 / 44 | 1,306k / 3,551k | 6,339 / 13,259 | 77 / 147 | 0 | 0 | 0 | 0 | 0 |
| T7 triage of 12 reports | 12 / 33 | 4 / 14 | 366k / 1,006k | 2,159 / 5,899 | 27 / 56 | 3 `label` (exit 3, 0, 0) | 2 | 36 | 1 | 0.00016 |
| T8 the tool for a job | 12 / 25 | 4 / 11 | 347k / 747k | 1,492 / 3,449 | 24 / 36 | `route` | 11 | 32 | 0 | 0.00244 |
| **total** | **130 / 244** | **52 / 106** | **4.25M / 8.32M** | **23.2k / 37.7k** | **302 / 421** | **9** | **21** | **84** | **1** | **0.0049** |

Turns, tool uses and tokens are the harness's per-run usage of the agent model, wall time is
first to last transcript timestamp. Requests, questions, cache hits and cost are read from the
jevify envelope of every call of that task (`meta.requests`, `meta.telemetry.semantic_questions`,
`meta.cache_hits`, `meta.cost_usd` at the configured input price, 0.042 USD per Mtok; every
estimate reports `complete: true`). Cache hits are counted apart: one, T7's third `label`, a
repeat of its second within the same run; every other call was a live request against a fresh
cache. When the agent did not ask for a machine format (5 of 9 calls), the wrapper ran the same
call with `--json` first (and `--dry-run` for `fill`), recorded that envelope, and let the agent's
own call replay the answers from the run's cache; the requests and cost in the table are the
first call's, the agent saw one command's latency plus a replay of a few milliseconds. The
whole with arm cost half a cent of backend inference; T8's `route` (11 requests, 32 questions
over the installed-tool inventory) is half of it.

The with arm spends 53 % of the turns, 49 % of the tool uses and 51 % of the input tokens of the
without arm. Four pairs carry the gap (T6, T7, T4, T8); in T4, T7 and T8 the jevify call replaced
the reading loop (T7 without read all twelve reports, T8 without read `vhs --help` and its man
page five ways), and in T6 neither arm called jevify: the with arm simply stopped after 17 tool
uses and the without arm after 44, on the same answer. T5 is the one pair where the with arm cost
more; it did not call jevify. Eight pairs, one run each: figures, not an effect, and one of the
four large gaps (T6) has nothing to do with jevify.

### What the second run adds and what it still cannot say

- Isolation is enforced by the OS for every command that goes through the wrapper, and the scan
  found no write outside any run; the wrapper is the agent's to skip (1 of 118 calls did, an
  `echo`), and the key stays readable to the with arm's shell.
- Cost is per pair, with cache hits apart: 21 requests, 84 questions, one cache hit, 0.0049 USD
  for the whole with arm; the without arm's backend cost is zero by construction.
- The with arm answers 7 of 8 again, for a different reason than in the first run: 0.8.3's `why`
  quoted the wrong line on T3 and 0.9.1 quotes the right one; 0.9.1's `label --files` under the
  sandbox labelled from file names alone on T7. Adoption is 5 of 8 instead of 4 of 8; the extra
  is T8's `route`.
- The sandbox itself changes the task: the with arm's T7 miss is a jevify defect the sandbox
  exposed, not a model decision, and a harness without that profile would not see it. Both are
  reported because both are true of an agent running jevify inside a deny-listed sandbox.
- The gold is still the evaluator's, one run per cell, eight pairs.

## The four items folded in from `hunch-lpp` (first run)

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

- Both arms answer 8 of 8 (the with arm 7 of 8: on the quoted line of T3 in the first run, on
  T7's labels in the second). Adoption is not effectiveness: nothing here shows jevify changed an
  outcome for the better.
- One run per cell, eight pairs, twice. The turn and token gaps are dominated by three or four
  pairs, and one large gap of the second run (T6) involves no jevify call.
- First run only: the isolation between arms is by construction, not enforced, and two of the
  six calls were cache hits, so its backend cost is understated. The second run enforces the
  sandbox per command and counts its one cache hit apart, with the two limits stated there.
- The finalist-miss figure is a reconstruction of round one, because no hook exposes it.

## Beads filed

First run: `hunch-93l` (P2, `pick` prefers a documentation page to the source file a description
names), `hunch-8b3` (P3, no hook exposes round-one ranks or the finalist set), `hunch-n35` (P3,
`why` points at the panic header rather than the line that explains the failure), `hunch-63r`
(P3, the unsure band never fires: `filter` reads "the record does not say" as a confident no).
The first three shipped in 0.9.0 and 0.9.1 and the second run reflects them.

Second run: `hunch-3fj` (P2, `label`/`pick --files`: a file whose excerpt cannot be read is
labelled from its name alone, confidently and silently, `excerpts_withheld` stays 0),
`hunch-x4w` (P2, `fill @{branch:}` resolves a remote-only branch to its bare name, which git
cannot resolve; the status line shows `origin/…` but the command gets the bare name).

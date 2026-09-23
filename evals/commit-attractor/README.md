# Is one commit an attractor for the `commit` kind?

Twenty-six descriptions of commits of this repository, spread over its 248-commit history, plus
one probe phrasing quoted from the observation this set answers. Each runs `fill` on both
backends with the answer cache off, and the finalists of round one recorded next to the
decision. A `branch` control runs the same way over the 35 refs of the same repository.

Measured 2026-09-22 with jevify 0.9.3 built from `cd49bcf` (`cargo build --locked`, debug),
answering model `jev-1.13.0` on both backends, on a macOS dev machine. Every run set
`JEVIFY_NO_CACHE=1` and `JEVIFY_DECISION=round_one`; every run used `--dry-run`, so no filled
command was executed.

## How a description was written without its subject line

`dump_diffs.py` writes each target's diff with `git show --format=`, which emits the patch and
nothing of the commit message: no subject, no body, no author, no date. The description author
read only those files (`diffs/*.diff`, through a stat-plus-hunks digest of them) and wrote the
description from the change itself. No description was written, reread or adjusted after a run;
`cases.jsonl` was complete before the first request. The one exception is declared: the case
`probe-subject-085` is not a written description but the phrasing the observation quotes
verbatim, which nearly quotes its target's own subject.

Two limits of the method, stated rather than hidden. A diff that edits `CHANGELOG.md` shows the
changelog prose of its own release, so for the six version-stamping commits the description is
written from text the commit itself introduced. And a commit's subject may still appear inside
another commit's diff (the tracker file `.beads/issues.jsonl` carries issue titles); no target's
own subject appears in its own diff. Targets were chosen as an anchor every tenth commit from
HEAD, moved forward to the nearest commit that changes something other than
`.beads/issues.jsonl`, so a target is a substantive change and not tracker churn. Files in
`diffs/` that a later selection dropped are left in place as one-line stubs.

## The files

| File | What it holds |
|:---|:---|
| `cases.jsonl` | the 27 commit cases: `id`, `target` short sha, `index` from HEAD, and the `description` |
| `branch-cases.jsonl` | the 14 `branch` control cases over the same repository |
| `diffs/*.diff` | each target's message-free diff, the only thing the description author read: the stat, then the first 400 lines of the patch, each line cut at 300 characters |
| `selection.tsv` | index from HEAD, full sha and short sha of every target |
| `pools/subjects-*.tsv` | `sha<TAB>subject` for the newest 60 and for all 248 commits, for the pool probe |
| `run.py` | runs one case file on one backend and writes one JSON row per case |
| `score.py` | coverage, accuracy, repeated wrong answers, finals recall, recall by window |
| `pool_probe.py` | the same descriptions over the `-` kind at pool 60 and pool 248 |
| `repeats.py` | three descriptions three times each, and two descriptions that fit no commit |
| `runs/*.jsonl` | every run, with the gate scores, the window count, the finalists and the answering model |

## What the runs say

The `commit` kind over the 248 commits of this repository, one row per backend.

| | keyless (classifier.dev) | TypeSafe |
|:---|---:|---:|
| cases | 27 | 27 |
| windows per case | 3 | 2 |
| requests per case | 4 | 3 |
| decided | 13 | 19 |
| right first | 11 | 14 |
| wrong | 2 | 5 |
| abstained | 14 (11 ambiguous, 3 no_match) | 8 (5 ambiguous, 3 no_match) |
| accuracy on the decided | 11/13 | 14/19 |
| hit@1 over every case | 11/27 | 14/27 |
| target reached the finals | 21/27 | 23/27 |

Of the 26 descriptions written from a diff alone, 11 are right on the keyless backend and 13 on
TypeSafe. The probe phrasing the observation quotes, `keyless batches of at most 60 records`,
returns its own target `7bcf70c` at 0.99 on TypeSafe and abstains `no_match` keyless.

### No commit is an attractor

No wrong answer repeats. Every wrong answer on both backends is a different commit:
keyless `dadfe99`, `1128d46`; TypeSafe `cb0547f`, `f0e3c91`, `62680bc`, `1128d46`, `7855eb0`.
Each is a near neighbour of its target, not a fixed destination.

`332191a` is returned for exactly one description on each backend: the one written from its own
diff, at 0.74 keyless and 0.91 on TypeSafe, and three repeats hold it (0.66, 0.62, 0.81 keyless;
0.85, 0.85, 0.63 on TypeSafe). It is the right answer there and appears nowhere else.

### Where the misses are

Recall by the window the target falls in (99 candidates per window keyless, 200 on TypeSafe):

| Backend | window 1 | window 2 | window 3 |
|:---|---:|---:|---:|
| keyless | 4 of 12 | 7 of 10 | 0 of 5 |
| TypeSafe | 14 of 22 | 0 of 5 | — |

The oldest window is 0 of 5 keyless and 0 of 5 on TypeSafe. Those targets are the oldest commits
in the history, from before the project was renamed; their subjects speak of `hunch`, their
bodies are empty, and their changed paths are the generic `src/` files that many commits touch.

The shortlist drops the target before the finals in 6 of 27 cases keyless
(`commit-005`, `commit-056`, `commit-135`, `commit-217`, `commit-235`, `commit-245`) and in 4 of
27 on TypeSafe (`commit-125`, `commit-135`, `commit-235`, `commit-245`). When the target does
reach the finals, the keyless run gets it right 11 times, abstains 9 times and answers wrong
once; TypeSafe gets it right 14 times, abstains 7 times and answers wrong twice. The dominant
keyless failure is therefore an abstention with the right commit sitting in the finals, not a
wrong commit.

### The `branch` control

The same repository, the same shortcut over names, 35 refs in one window:

| | keyless | TypeSafe |
|:---|---:|---:|
| cases | 14 | 14 |
| right first | 12 | 12 |
| wrong | 0 | 0 |
| abstained | 2 | 2 |

Thirteen of the fourteen names are opaque (`wt/bead-NN`) and the fourteenth is `main`, so the
names round carries almost nothing and the tier-two evidence decides. It decides well.

### Pool size against evidence

`pool_probe.py` holds the evidence fixed at one `sha<TAB>subject` line per commit (the `-` kind,
which has no tier-two round) and varies only the pool, over the seven cases whose target is among
the newest 60 commits, so the answer is in both pools:

| Backend | pool 60 | pool 248 |
|:---|---:|---:|
| keyless | 2 of 7 | 2 of 7 |
| TypeSafe | 4 of 7 | 2 of 7 |

Cutting the pool from 248 to 60 changes nothing keyless and moves TypeSafe by two cases. The same
seven cases under the real `commit` kind, which adds the body and the changed paths in a finals
round, are 4 of 7 keyless and 5 of 7 on TypeSafe. The tier-two round, not the pool size, is what
makes the kind work.

### A description that fits no commit

`rewrote everything in Go` and `ports the user interface to Android` fit nothing in this history.
Keyless abstains on both (`ambiguous`, then `no_match`). TypeSafe abstains on the second and
answers the first with `041e6d1` ("feat: scaffold hunch CLI and dispatch") in four runs of four,
at 0.72, 0.52, 0.71 and 0.73 against none 0.20–0.26. A scaffolding commit does create a whole
program at once in one language, so the first description was matchable and the demonstration
was asking the wrong question. `docs/demo/examples.sh` now runs the second one, which holds none
at 1.00 on TypeSafe in all four runs (`nothing-fits/`).

## The finding

The observation this set was built to check does not reproduce. There is no attractor: no wrong
commit is returned for more than one description, on either backend, and `332191a` is returned
only for the description of its own diff.

Of the four explanations the question offered, the evidence supports the first, with a measured
qualification, and rules out the other three:

- **Supported: the kind is sound and the descriptions were weak.** With descriptions written from
  the diffs, the keyless backend answers 11 of 27 right and never repeats a wrong commit, and
  TypeSafe answers 14 of 27. The `branch` control on the same repository is 12 of 14 on both
  backends. The qualification is that "sound" here means honest rather than strong: the keyless
  run abstains on 14 of 27 cases, 9 of them with the right commit already in the finals.
- **Ruled out: the shortlist drops the right commit before the finals.** It happens, but in 6 of
  27 cases keyless and 4 of 27 on TypeSafe, and it is not what the abstentions are made of.
- **Ruled out: the evidence a commit carries is too thin to tell commits apart.** It is thin for
  the oldest window, which is 0 of 5 on both backends, and it is decisive elsewhere: the finals
  round takes the same seven cases from 2 of 7 to 4 of 7 keyless at a fixed pool.
- **Ruled out: the free backend cannot do it at this pool size.** Keyless and TypeSafe fail on the
  same window and succeed on the same kinds of case; keyless costs its accuracy in abstentions,
  not in wrong answers. Cutting the pool from 248 to 60 at fixed evidence changes keyless not at
  all.

What a follow-up should change is not the `commit` kind's shortlist but two smaller things: the
nothing-fits demonstration in `docs/demo/examples.sh`, which reproducibly returns a commit on
TypeSafe, and any documentation that implies the `commit` kind reaches a decision on most
descriptions — keyless it reaches one on 13 of 27.

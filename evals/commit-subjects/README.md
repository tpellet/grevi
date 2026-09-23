# When the subject line lies

Twenty descriptions of commits whose subject does not hold the change the description asks
for, in two repositories that are not this one. This repository's own subjects are honest, so
its history flatters the `commit` kind; these two do not.

Measured 2026-09-23 with jevify 0.10.0, debug builds of the commit before and after the change
this set paid for, answering model `jev-1.13.0` on both backends, on a macOS dev machine. Every
run sets `JEVIFY_NO_CACHE=1` and `JEVIFY_DECISION=round_one` and uses `fill --dry-run`, so no
answer is reused and no filled command is executed.

## The two halves

**The scratch repository** (`make_repo.sh`, 21 commits). Ten pairs. Each pair is a
documentation-only commit whose subject announces a change — `fix: payment gateway timeout` —
and a code commit whose subject says nothing — `chore: tidy imports in payments` — and which
holds the change the first one announces. The target of every case is the code commit. Commits
are unsigned and dated from a fixed clock, so `make_repo.sh` reproduces the shas in
`cases-scratch.jsonl` exactly.

**ripgrep** (`cases-ripgrep.jsonl`, 2,287 commits at `3fce3b5bb0236da2df6d99672afb8a719642eca7`).
Ten real commits whose subjects say `Update types.rs`, `style`, `Tweak how binary files are
handled internally.` and the like, while the patch closes an unbalanced glob bracket, drops a
dependency, or rewrites a binary-detection heuristic. Selection read the subjects — the set is
defined by dull subjects — but every description is written from the patch, and no description
repeats a word of its target's subject.

```
git clone https://github.com/BurntSushi/ripgrep rg && git -C rg checkout 3fce3b5bb0236da2df6d99672afb8a719642eca7
bash make_repo.sh /path/to/liars
JEVIFY_BIN=target/debug/jevify python3 run.py classifier cases-scratch.jsonl /path/to/liars runs/after-scratch-classifier.jsonl
python3 score.py runs/*.jsonl
```

## The files

| File | What it holds |
|:---|:---|
| `make_repo.sh` | builds the scratch repository of ten lying pairs |
| `cases-scratch.jsonl` | the 10 scratch cases: `id`, `target` (the code commit), `claimant` (the commit whose subject lies), `description` |
| `cases-ripgrep.jsonl` | the 10 ripgrep cases at the pinned sha |
| `run.py` | runs one case file against one repository on one backend; the binary comes from `JEVIFY_BIN`, so two builds run the same cases |
| `score.py` | hit@1, how often the lying subject won, confidence on wrong answers, whether the target reached the finals, requests and seconds |
| `runs/*.jsonl` | every run, before and after, on both backends, including the `commit-attractor` cases rerun on both builds |

## What the runs say

`before` is the build where a `commit` finalist carried its subject, body and changed paths and
the subjects round could decide alone. `after` is the build where the finalist also carries its
diffstat and the first 1,000 characters of its patch, the subjects round never decides a
`commit` alone, and a single-window `commit` finals keeps the candidates the subjects scored
0.00.

### The scratch repository, 21 commits in one window

| | keyless before | keyless after | TypeSafe before | TypeSafe after |
|:---|---:|---:|---:|---:|
| right | 0 of 10 | **10 of 10** | 0 of 10 | **10 of 10** |
| wrong | 10 | 0 | 10 | 0 |
| the lying subject won | 10 | 0 | 10 | 0 |
| confidence when wrong | 0.93–1.00 | — | 0.82–1.00 | — |
| requests per case | 1 | 2 | 1 | 2 |
| seconds per case, median | 0.62 | 2.40 | 0.30 | 1.76 |

The `before` column is the failure in full: ten confident wrong answers out of ten, on both
backends, with the runner-up at 0.00. The subjects round decided every case by itself, which is
why `before` costs one request: the finals never ran, so no evidence a finals round could carry
would have been read.

### ripgrep, 2,287 commits in 24 keyless windows and 12 TypeSafe windows

| | keyless before | keyless after | TypeSafe before | TypeSafe after |
|:---|---:|---:|---:|---:|
| right | 6 of 10 | 6 of 10 | 7 of 10 | 8 of 10 |
| wrong | 1 | 1 | 0 | 1 |
| abstained | 3 | 3 | 3 | 1 |
| requests per case | 25 | 25 | 13 | 13 |
| seconds per case, median | 4.43 | 5.15 | 1.59 | 2.29 |

A pool this size runs many windows, so the subjects round never decided alone and the request
count does not move. The patch converts abstentions into answers on TypeSafe (3 to 1) and holds
keyless flat. `rg-02` is wrong on both backends and both builds: its description of an
`--ignore-file` help-text rewrite fits a later commit that clarified the same flag.

### The regression on this repository's own history

The 27 cases of `evals/commit-attractor/` rerun on both builds, same descriptions, same pool:

| | keyless before | keyless after | TypeSafe before | TypeSafe after |
|:---|---:|---:|---:|---:|
| right | 9 of 27 | 12 of 27 | 14 of 27 | 22 of 27 |
| wrong | 4 | 3 | 5 | 4 |
| abstained | 14 | 12 | 8 | 1 |
| requests per case | 4 | 4 | 3 | 3 |

The patch costs nothing here and pays: keyless gains three right answers and loses one wrong
one, TypeSafe gains eight right answers and loses one wrong one, at the same request count. The
dominant keyless failure the `commit-attractor` set named — an abstention with the right commit
already in the finals — is what the patch spends itself on.

### What the window holds

The keyless finals window budgets 30,000 characters over at most 24 finalists, and clips each
finalist to 1,250. Over the newest 24 commits of ripgrep, a finalist's body, paths and clipped
diff average 1,184 characters and total 28,434 of the 30,000. Ten of the 24 exceed the per-item
clip and lose the tail of their patch; the diffstat leads the diff, so a clipped finalist still
shows every file it touched and its line counts. No finalist is dropped: the clip is per item,
not a queue, so all 24 fit at 99 candidates per window and at any pool size.

## The finding

Giving the finals round the patch is the cheaper of the two options the failure allows. It costs
one extra request only on a repository small enough to fit one window, where the subjects round
used to decide alone; on a real pool it costs no requests at all and about 0.7 seconds of local
`git show`. It buys 0 of 20 to 20 of 20 on the lying-subject halves, on both backends, and it
does not cost a case on the honest history that motivated the earlier measurement — it gains
there too.

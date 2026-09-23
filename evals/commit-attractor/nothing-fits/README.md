# Which description this history genuinely cannot match

The nothing-fits demonstration of `docs/demo/examples.sh` asks the `commit` kind for a commit that
this repository never made. The demonstration is only honest if it abstains everywhere, so eight
candidate descriptions run the whole history on both backends, four times each, and the three
numbers the decision reads — the best candidate, the next candidate and NONE — are recorded for
every run.

Measured 2026-09-23 with jevify 0.9.3 built from `67ac2e7` (`cargo build --locked`, debug),
answering model `jev-1.13.0` on both backends, over the 250 commits of `HEAD`, on a macOS dev
machine. Every run sets `JEVIFY_NO_CACHE=1` and uses `fill --dry-run`, so no answer is reused and
no filled command is executed.

## The files

| File | What it holds |
|:---|:---|
| `run.py` | the eight descriptions, four repeats each on both backends |
| `runs/phrases.jsonl` | one line per run: the reason, the answer if any, `best`, `next`, `none` and `any` |

## What the runs say

Abstentions out of four, with the range of `best` and of `none` across the four runs.

| Description | keyless abstains | keyless best | keyless none | TypeSafe abstains | TypeSafe best | TypeSafe none |
|:---|---:|---:|---:|---:|---:|---:|
| rewrote everything in Go | 4/4 | 0.34–0.43 | 0.26–0.31 | **1/4** | 0.10–0.71 | 0.21–0.83 |
| ports the user interface to Android | 4/4 | 0.08–0.13 | 0.54–0.68 | 4/4 | 0.00 | 1.00 |
| migrates the database from Oracle to Postgres | 4/4 | 0.16–0.23 | 0.49–0.58 | 4/4 | 0.00 | 1.00 |
| adds Kubernetes operator manifests for the staging cluster | 4/4 | 0.11–0.17 | 0.53–0.61 | 4/4 | 0.00 | 1.00 |
| translates the user manual into Japanese | 4/4 | 0.12–0.27 | 0.50–0.64 | 4/4 | 0.04–0.06 | 0.94–0.96 |
| fixes the memory leak in the video decoder | 4/4 | 0.15–0.27 | 0.43–0.66 | 4/4 | 0.00 | 1.00 |
| adds billing and subscription management to the web dashboard | 4/4 | 0.08–0.15 | 0.62–0.76 | 4/4 | 0.00 | 1.00 |
| ships the iOS app to the App Store | 4/4 | 0.08–0.18 | 0.48–0.73 | 4/4 | 0.01–0.03 | 0.95–0.97 |

Seven of the eight abstain in every run on both backends. `rewrote everything in Go` is the one
that acts: on TypeSafe it returns `041e6d1` ("feat: scaffold hunch CLI and dispatch") in three runs
of four, at `best` 0.67, 0.68 and 0.71 against `none` 0.21 to 0.24.

## The finding

The description is the problem, not the gate. A scaffolding commit that creates a Rust binary from
nothing is a real, if weak, neighbour of "rewrote everything in Go": both describe a whole project
appearing at once in one language, and the model answers with the margin it would give a genuine
match, not a marginal one. Every description that this history truly cannot match drives `none` to
0.94 or above on TypeSafe and above `best` on the keyless backend, and the decision abstains
without the ratio rule ever being close.

`ports the user interface to Android` is the description the demonstration uses. jevify is a
command-line program with no user interface of any kind and no mobile target, so no commit of this
history can approach it in language or in changed paths; TypeSafe puts the whole probability mass
on NONE in all four runs, and the keyless backend answers `no_match` in all four.

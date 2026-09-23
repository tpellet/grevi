# The frozen validation set

A validation set for the verbs jevify has: `fill` (per kind), `pick`, `pick --from`, `why`,
`filter`, `label`, `is` and `route`. Every case comes from a task family outside this repository,
carries its source and licence, and has an adjudicated gold answer that two independent annotators
labelled first. The gold lives apart from the manifest a run reads, so a run cannot see the answer
it is judged against.

## The files

| File | What it holds |
|:---|:---|
| `families.jsonl` | one line per family: `id`, `split`, `kind`, `source`, `license`, the pinned commit where there is one, how the material was collected, and the verbs it feeds |
| `cases.jsonl` | the manifest a run reads: `id`, `verb`, `family`, `split`, `argv`, the input it reads with that input's `sha256`, the answer space, and for the repository kinds the pinned `env`. It carries no answer |
| `gold/gold.jsonl` | the gold: `id`, `gold`, both annotators' labels, whether adjudication was needed and on what ground |
| `gold/disagreements.md` | every case the annotators labelled differently, and how each one was resolved |
| `annotator-instructions.md` | the sheet both annotators worked from, and the one a third annotator would use |
| `inputs/why/*.log` | the failing and passing job logs, normalised, with a `*.meta.json` naming the run they come from |
| `inputs/records/*.txt` | the record sets `pick`, `filter` and `label` read on stdin |
| `inputs/context/*.md` | the documents `is` reads with `--context` |
| `inputs/candidates/*.tsv` | the frozen candidate lists `fill` reads with `--candidates` on the `-` kind |
| `inputs.sha256.json` | the digest of every input file, for a second check outside the manifest |

`scripts/validation_gold.py check` validates the manifest before any request is paid: every input
exists and hashes as recorded, every family is declared, a case's split matches its family's, and
no case carries its own answer. `scripts/validation_gold.py score <run.jsonl>` reads the gold and
reports coverage, abstentions and false actions per backend and verb.

## Splits

A family is the sampling unit: one repository at a pinned commit, one project's CI logs, or one
tool suite. No family appears in both splits, and no family is this repository or the synthetic
panel of 2026-09-20. The calibration split is where a threshold may be chosen; the validation
split is only ever scored.

## Two kinds of case

- **Self-contained.** The bytes the verb reads are in `inputs/`, and `cases.jsonl` records their
  digest. `pick`, `why`, `filter`, `label`, `is` and the `-` kind of `fill` are all of this kind.
- **Environment-pinned.** The candidates come from a lister, so the case pins the public
  repository and the commit (`env`) instead of freezing the list. The runner clones that
  repository at that commit and runs the verb inside it. A checkout that does not match the pin is
  reported as `not_run`, never scored.

`route` reads the machine's own PATH, so each case names the tools it requires in `requires`; a
case whose tools are absent is `not_run`.

`argv` is what follows `jevify`. A case with `stdin` pipes that file in; a case with `candidates`
passes it as `--candidates FILE`; a case with `context` replaces the literal `CONTEXT` in `argv`
with that file's path. `fill` cases run with `--dry-run`, so nothing a case describes is ever
executed.

## The answer space

Every verb can answer that nothing fits, and the gold says so where that is the right answer:

| Verb | A decision | Abstention | Gold that means "do not act" |
|:---|:---|:---|:---|
| `fill`, `pick`, `pick --from` | the handle (path, short sha, or 1-based line) | `none` | `none`, `ambiguous` |
| `why` | the 1-based line of the root cause | `none` | `none`, `ambiguous` |
| `filter` | `keep` or `drop` per record | `unsure` | `unsure` |
| `label` | one of the labels | `?` | `?` |
| `is` | `yes` or `no` | `unsure` | `unsure` |
| `route` | the tool name | `none` | `none`, `ambiguous` |

An abstention on a case whose gold is `none`, `ambiguous` or `unsure` is right, not a miss. A
decision on such a case is a false action, as is a decision that differs from a concrete gold.

## Annotation

Two annotators labelled every case independently, from the case's input alone, neither of them the
author of any verb and neither seeing the other's labels or any intended answer. Annotator A is a
Claude Sonnet subagent, annotator B a Codex run on gpt-6-astra with a read-only sandbox; both
worked from the same written instruction sheet. Disagreements were adjudicated afterwards by
reading the input again; `gold/disagreements.md` lists every one with its resolution. No backend
and no jevify run took part: the gold is defined by two language models working from a written
instruction sheet, and by the adjudication of their disagreements, not by the product under test.

## What is in the set

153 cases. "Abstention gold" counts the cases whose right answer is `none`, `ambiguous`, `unsure`
or `?`. A `filter` or `label` case is one record of a run: the runner executes the run once and
scores each of its records.

| Verb | Calibration cases (families, abstention gold) | Validation cases (families, abstention gold) |
|:---|:---|:---|
| `fill` | 5 (2 families, 2) | 7 (2 families, 2) |
| `pick` | 6 (2 families, 2) | 6 (3 families, 1) |
| `pick --from` | 4 (2 families, 1) | 5 (2 families, 1) |
| `why` | 9 (8 families, 1) | 12 (6 families, 3) |
| `filter` | 20 (2 families, 0) | 20 (2 families, 4) |
| `label` | 16 (2 families, 5) | 16 (2 families, 4) |
| `is` | 7 (2 families, 1) | 8 (2 families, 1) |
| `route` | 6 (2 families, 1) | 6 (2 families, 1) |

The two annotators gave the same label on 141 of the 153 cases; the 12 that differ are adjudicated
in `gold/disagreements.md`. Four of them are `why` cases where both lines fall inside the same
root-cause block, which the gold records as a range.

## Scoring a run

A run writes one JSON object per case and scores it with `scripts/validation_gold.py score`:

```json
{"id": "pick-val-01", "backend": "typesafe", "model": "jev-1.13.0", "threshold": 0.5,
 "decision": "6", "score": 0.91, "exit_code": 0}
```

`backend` separates TypeSafe from classifier.dev, `score` is the value at the gate, and
`decision` is the verb's answer in the answer space above; a case the run could not execute is
`"decision": "not_run"`. The scorer prints, per backend and verb: how many cases were scored,
coverage (decided over scored), accuracy on the decided ones, the abstention rate, the number of
false actions, and the number not run. A `why` gold is the inclusive line range of the root-cause
block, so a decision inside the block is correct; a `route` gold may accept several commands.

## Provenance and licence

Every family names its source and its licence in `families.jsonl`. The repository material is the
projects' own files, paths and commit subjects under MIT or Apache-2.0; the logs are those
projects' build and test output, reproduced under the same licences for evaluation, normalised the
way `evals/why/README.md` describes (the job or step prefix, the ANSI escapes and the runner's own
`##[…]` lines removed, blank runs collapsed, a window of at most 300 lines kept). No case holds a
credential, an email address, a personal path, or text from outside those licences: the `route`
families store tool names only.

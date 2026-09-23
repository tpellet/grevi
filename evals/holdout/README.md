# The source-held-out set

103 cases over the five verbs of the 0.9.x behaviours — `fill`, `pick`, `why`, `filter` and
`route` — drawn from sources that no other set under `evals/` and no measurement in
`benchmarks/` uses. Every source is pinned: a public repository by its commit, a failure log by
the command and toolchain that produced it, a tool inventory by the digest of the frozen file.
The gold lives apart from the manifest a run reads, so a run cannot see the answer it is judged
against.

## The files

| File | What it holds |
|:---|:---|
| `sources.jsonl` | one line per source: `id`, `kind`, where it comes from, its `pin`, its licence, how the material was collected, and the verbs it feeds |
| `cases.jsonl` | the manifest a run reads: `id`, `verb`, `source`, `argv`, the input it reads with that input's `sha256`, the answer space, and for the repository cases the pinned `env`. It carries no answer |
| `gold/gold.jsonl` | the gold: `id`, `verb`, `gold`, both passes' labels with their confidence and notes, whether the two agreed, and the ground of adjudication where they did not |
| `gold/pass-a.jsonl`, `gold/pass-b.jsonl` | the two labelling passes as they were written, before adjudication |
| `gold/adjudication.md` | every disagreement with its resolution, and the method both passes worked under |
| `inputs/why/*.log` | the failure logs, each with a `*.meta.json` naming the command, the toolchain and the exit code that produced it |
| `inputs/records/*.txt` | the record sets `pick` and `filter` read on stdin |
| `inputs/candidates/*.tsv` | the frozen candidate list the `-` kind of `fill` reads with `--candidates` |
| `inputs/inventory/*.json` | the tool inventories `route` reads through `JEVIFY_INVENTORY_FILE` |
| `inputs.sha256.json` | the digest of every input file, for a second check outside the manifest |
| `make_why_logs.py` | the script that produces the `why` logs by running real commands on projects it writes in a temporary directory |

`scripts/holdout_score.py check` validates the manifest before any request is paid for: every
input exists and hashes as recorded, every source is declared, no case carries its own answer,
and the gold covers every case. `scripts/holdout_run.py` runs one backend over the set;
`scripts/holdout_score.py score <run.jsonl>` reads the gold and reports coverage, accuracy on the
decided cases, abstention correctness and false actions per backend and verb.

## What "source-held-out" means here

`evals/validation/families.jsonl` draws on pydantic-ai, cli/cli, davinci-resolve-mcp, ruff,
prometheus and the GitHub Actions logs of bevy, deno, rust-analyzer, svelte, tokio, fd, vite,
vitest, pytest, pydantic, helm and home-assistant, plus four suites of POSIX tools read off the
machine's own PATH. `evals/why/corpus.jsonl` draws on the Actions logs of zod, distribution,
helm, home-assistant, moby, nushell, ollama, black, pydantic, pytest, ratatui, fd, tokio, vite,
vitest and this repository. `evals/fill/finals/` draws on ripgrep, fzf and bat. `benchmarks/`
measures on `ls /usr/bin`, one small failing `cargo build` log and its own demo fixtures.

This set uses none of them:

| Source | Pin | Licence | Verbs |
|:---|:---|:---|:---|
| github.com/casey/just | `6119e6a` | CC0-1.0 | `fill`, `pick`, `filter` |
| github.com/dandavison/delta | `5ddd7fa` | MIT | `fill`, `pick` |
| github.com/ajeetdsouza/zoxide | `09a18b4` | MIT | `fill`, `pick`, `filter` |
| 22 failure logs produced by `make_why_logs.py` | rustc 1.93.1, Apple clang 21.0.0, GNU Make 3.81, CPython 3.14.7, serde `=1.0.228` | written for this set | `why` |
| three tool inventories of 24 names each | the digests in `cases.jsonl` | written for this set | `route` |

The three repositories appear in no other tracked file. Both of these return nothing:

```sh
git grep -n -e casey/just -e dandavison/delta -e ajeetdsouza/zoxide \
    -- evals benchmarks scripts docs README.md ':!evals/holdout' ':!scripts/holdout_*'
git grep -hoE 'github\.com/[A-Za-z0-9_.-]+/(just|delta|zoxide)\b' \
    -- evals benchmarks scripts docs README.md ':!evals/holdout' ':!scripts/holdout_*'
```

The second one also rules out the same three projects under another owner. The `why` logs cannot overlap `evals/why/corpus.jsonl`, whose every line is GitHub
Actions output from a public project, because each of these is the output of a command run here
on a project `make_why_logs.py` wrote itself. The `route` inventories replace the PATH rather
than reading it, and none of their 72 names is a tool the `route` cases of `evals/validation/`
ask for (`wc`, `head`, `tr`, `sed`, `tar`, `unzip`, `zipinfo`, `curl`, `dig`, `host`,
`nslookup`, `rsync`, `jq`, `gh`).

## The logs

A `why` log is the combined stdout and stderr of one command, through one pipe, the way a caller
writes `cargo test 2>&1 | jevify why`. The temporary directory becomes `/tmp/holdout`, the home
directory and the cargo registry become `$HOME` and `$CARGO_HOME`, elapsed times become `Xs`,
and a log longer than 120 lines keeps its last 120. The logs in `inputs/` are frozen and hashed:
running the script again produces equivalent logs, but a thread id or a test binary's hash makes
them different bytes, so the frozen copies are what the manifest and the gold speak about.

Twenty of the 22 fail; two succeed and carry only a warning or none, and their gold is `none`.

## Two kinds of case

- **Self-contained.** The bytes the verb reads are in `inputs/`, and `cases.jsonl` records their
  digest. Every `why`, `pick`, `filter` and `route` case and the `-` kind of `fill` are of this
  kind.
- **Repository-pinned.** The candidates come from a lister, so the case pins the repository and
  the commit (`env`) instead of freezing the list. `scripts/holdout_run.py --clone` checks the
  repositories out detached at their commits and runs the verb inside them; a checkout that does
  not match the pin is `not_run`, never scored. `inputs/records/zoxide-files.txt` and
  `inputs/records/delta-src-files.txt` are the listings at those pins, so the answer space of a
  repository-pinned case can be read without a clone.

`argv` is what follows `jevify`. A case with `stdin` pipes that file in; a case with
`candidates` passes it as `--candidates FILE`; a case with `inventory` sets
`JEVIFY_INVENTORY_FILE` to that file. `fill` cases run with `--dry-run` and `why` cases with
`--no-save`, so nothing a case describes is executed and no input is copied anywhere.

## The answer space

| Verb | A decision | Abstention | Gold that means "do not act" |
|:---|:---|:---|:---|
| `fill` | the handle (a path, or a short sha on the `-` kind) | `none` | `none`, `ambiguous` |
| `pick` | the 1-based line of the chosen record | `none` | `none`, `ambiguous` |
| `why` | the 1-based line of the root cause | `none` | `none`, `ambiguous` |
| `filter` | `keep` or `drop` per record | `unsure` | `unsure` |
| `route` | the tool name | `none` | `none`, `ambiguous` |

An abstention on a case whose gold is `none`, `ambiguous` or `unsure` is right, not a miss. A
decision on such a case is a false action, as is a decision that differs from a concrete gold. A
`why` gold is the inclusive line range of the root-cause block, so a decision inside the block is
correct. A gold of the form `{"any_of": [...]}` accepts any of the answers it names; adjudication
uses it where two answers are equally right.

## Annotation

Two passes label every case independently of any run, from the case's input alone, working from
`evals/validation/annotator-instructions.md`. Both passes are readings by the same Claude Opus
agent, the one that built the set: pass B labels each case again with pass A's file closed, but
one reader is not two annotators, and a bias that survives a second reading survives into the
gold. `gold/adjudication.md` says so, carries both labels for every case, and resolves the eight
cases the passes labelled differently by reading the input again. No backend and no jevify run
takes part: the gold is fixed before the first request.

## What is in the set

"Abstention gold" counts the cases whose right answer is `none`, `ambiguous` or `unsure`. A
`filter` case is one record of a run: the verb executes once per run and each of its records is
scored on its own.

| Verb | Cases | Sources | Abstention gold |
|:---|---:|---:|---:|
| `fill` | 21 | 3 | 3 |
| `pick` | 20 | 3 | 5 |
| `why` | 22 | 1 | 2 |
| `filter` | 20 | 2 | 2 |
| `route` | 20 | 1 | 4 |

The two passes gave the same label on 95 of the 103 cases; `gold/adjudication.md` resolves the
other 8.

## Running it

```sh
cargo build --release
python3 scripts/holdout_score.py check
python3 scripts/holdout_run.py --backend classifier --clone \
    --out evals/out/holdout/classifier.jsonl
python3 scripts/holdout_score.py score evals/out/holdout/classifier.jsonl
```

`--backend typesafe` reads the key from `TYPESAFE_API_KEY_FILE` in the environment; the runner
never opens that file. `--verb`, `--only` and `--limit` narrow a run. Every case runs with
`JEVIFY_NO_CACHE=1`, so every decision is a live answer, and each raw envelope is kept under
`<out dir>/raw/<backend>/<case id>.json`.

## Provenance and licence

The repository material is the projects' own paths and commit subjects, under CC0-1.0 for
`casey/just` and MIT for `dandavison/delta` and `ajeetdsouza/zoxide`, reproduced for evaluation.
The failure logs and the tool inventories are written for this set. No case holds a credential,
an email address, a personal path or another person's data: the logs are normalised as described
above and the inventories store tool names and one-line summaries only.

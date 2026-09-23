# The held-out content-phrase set for `fill`

Thirty-three `fill` cases over the `file` and `dir` kinds whose phrase says what a file or
directory does, not what it is called. They measure the finals path of `file` and `dir`: a
`file` or `dir` marker always runs its finals, and the shortcut (a decisive names round with the
runner-up name out of play decides alone) applies to `branch` and `commit` only, which this set
does not cover. The held-out measurement is in `benchmarks/results.md` under "The `fill` finals
always on for `file` and `dir` (measured 2026-09-22)". No case comes from this repository or
from a family of `evals/validation/`, and nobody read a case while the rule was written: the
phrases were written against the three repositories below and labelled afterwards.

| File | What it holds |
|:---|:---|
| `cases.jsonl` | the manifest a run reads: `id`, `kind`, the repository with its `source`, `pin` and `license`, the literal `prefix` that narrows the listing, the `phrase`, the `argv` after `jevify`, the frozen candidate `listing` with its `sha256` and its `candidates` count. It carries no answer |
| `inputs/*.txt` | the candidate listings, one per repository and scope: `git ls-files` under the prefix at the pin, and for `dir` the directories those files are in |
| `gold/gold.jsonl` | the gold: `id`, `gold`, both annotators' labels, and the ground of adjudication where they differed |
| `gold/disagreements.md` | every case the annotators labelled differently, and how it was resolved |

Every listing fits one window on both backends (at most 94 candidates), so every case takes the
single-window path, one names round and one finals. Each case runs `--dry-run` with `echo` as the
command, inside a clone of the repository at the pin (`scripts/eval_fill_finals.py run`);
nothing a case describes is executed.

| Repository | Pin | Licence | Scopes |
|:---|:---|:---|:---|
| github.com/BurntSushi/ripgrep | `3fce3b5` | MIT OR Unlicense | `crates/core/`, `crates/ignore/`, `crates/printer/` (file); the whole tree (dir) |
| github.com/junegunn/fzf | `b1be3a8` | MIT | `src/` (file); the whole tree (dir) |
| github.com/sharkdp/bat | `4987f76` | MIT OR Apache-2.0 | `src/` (file and dir) |

The answer space is a path from the listing, `none` when no candidate fits, or `ambiguous`
when two fit equally. Two annotators labelled every case independently from the listing and the
clone, working from `evals/validation/annotator-instructions.md`; annotator A is a Claude Sonnet
subagent and annotator B a Claude Opus subagent, neither of them the author of the phrases, and
no jevify run took part in the gold. `scripts/eval_fill_finals.py score` reports, per run and
backend, how often the finals ran, coverage, accuracy on the decided cases, abstentions and how
many of them were right, false actions, and the total number of requests.

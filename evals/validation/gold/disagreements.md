# Disagreements and how they were resolved

Annotator A is a Claude Sonnet subagent, annotator B a Codex run on gpt-6-astra. Each labelled
every case alone, from the case's input and the written instruction sheet, without seeing the
other's labels and without any intended answer to copy. Where they differ, the adjudicator read
the input again and decided; the ground of every decision is below, and both labels stay in
`gold.jsonl` next to the adjudicated one.

They gave the same label on 141 of 153 cases (92.2 %). A `why` case whose two lines fall inside the same
root-cause block is listed here as well: the labels differ, the block they point into does not.

## why-cal-npm-04 — `why`

- Task: -n 1
- A: `257` · B: `260` · adjudicated: **`[258, 269]`**
- Ground: the block that names the failure runs from the `FAIL specs/runner.test.ts > timeout hooks` header through the expected/received diff. Annotator A pointed at line 257, the blank line above the header, which is outside the block; annotator B's 260 is inside it.

## why-cal-pytest-02 — `why`

- Task: -n 1
- A: `257` · B: `258` · adjudicated: **`[257, 265]`**
- Ground: the pytest failure block is the `>` statement line, its `E` lines and the closing `test_collection.py:1884: AssertionError`. Both annotators pointed inside it (257 and 258); the gold is the block.

## why-val-prometheus-prometheus-33824930195 — `why`

- Task: -n 1
- A: `292` · B: `293` · adjudicated: **`[292, 297]`**
- Ground: the block runs from `✗ Lockfile failed supply-chain policy check` through the four offending entries. Both annotators pointed inside it (292 and 293); the gold is the block, and `make: *** Error 1` stays out of it as a summary.

## why-val-sveltejs-svelte-35722706116 — `why`

- Task: -n 1
- A: `280` · B: `281` · adjudicated: **`[280, 283]`**
- Ground: the block is the `FAIL packages/svelte/tests/signals/test.ts` header, the `Error: Test timed out in 30000ms.` line and its hint and location. Both annotators pointed inside it (280 and 281).

## filter-val-ruff-04 — `filter`

- Task: the change is about the ty type checker
- Record: `67b3bd4 Update rules table with category information (#28651)`
- A: `drop` · B: `unsure` · adjudicated: **`"drop"`**
- Ground: every ty change in this record set carries the `[ty]` prefix and this one does not, so the record itself settles it: the rules table it updates is the linter's. A careful reader can decide, so `unsure` is not the right answer.

## label-cal-pai-07 — `label`

- Task: feature,fix,docs
- Record: `28961b0 Run the harness navigation check when a pull request leaves draft (#8503)`
- A: `feature` · B: `?` · adjudicated: **`"?"`**
- Ground: the record changes when a CI check runs. That is neither a product feature, nor a fix, nor documentation, and the label set offers no third option, so `?` is the honest answer.

## label-val-ruff-01 — `label`

- Task: bugfix,documentation,performance
- Record: `97acb93 [ty] Skip reading notebooks when discovering scripts (#28781)`
- A: `bugfix` · B: `performance` · adjudicated: **`"?"`**
- Ground: skipping notebooks while discovering scripts reads as a correctness fix and as a saved read at once; the subject line does not separate bugfix from performance, so two labels fit equally.

## label-val-ruff-02 — `label`

- Task: bugfix,documentation,performance
- Record: `4d322b8 [ty] Filter diagnostics before computing result IDs (#28759)`
- A: `performance` · B: `?` · adjudicated: **`"?"`**
- Ground: filtering before computing result IDs can be a correctness ordering fix or an efficiency change; nothing in the record decides, so no single label fits.

## route-cal-arch-02 — `route`

- Task: list what is inside a zip file without unpacking it
- A: `unzip` · B: `ambiguous` · adjudicated: **`{"any_of": ["unzip", "zipinfo"]}`**
- Ground: `unzip -l` and `zipinfo` both list a zip's contents without extracting, and either serves the task, so the gold accepts both. `ambiguous` is reserved for a task no single answer serves.

## route-val-net-02 — `route`

- Task: look up the IP address a hostname resolves to
- A: `dig` · B: `ambiguous` · adjudicated: **`{"any_of": ["dig", "host", "nslookup"]}`**
- Ground: all three resolve a hostname to an address and any of them answers the task; the gold accepts all three rather than demanding an abstention.

## filter-cal-cli-x-02 — `filter`

- Task: the change is a bug fix
- Record: `9eb4db2 Merge pull request #14461 from cli/bump-go-gh`
- A: `drop` · B: `unsure` · adjudicated: **`"drop"`**
- Ground: the record names what the change is: a merge of a branch that bumps the go-gh dependency. A version bump described as a bump is not a bug fix, which is what the two agreed dropped records in the same run turn on, so the record decides and `unsure` is not needed.

## filter-cal-cli-x-04 — `filter`

- Task: the change is a bug fix
- Record: `9909db8 Merge pull request #14442 from cli/bump-go-1.27.1`
- A: `drop` · B: `unsure` · adjudicated: **`"drop"`**
- Ground: same ground as filter-cal-cli-x-02: the record describes a Go toolchain bump, which is not a bug fix.

# How it works

## Selection and NONE

jevify selects existing records, installed tools, tracked hunks and destination folders. The
model writes no answer text. `fill` substitutes listed handles or caller-written options, then
becomes the caller's command. `why` adds line numbers and context; `pick` and `filter` preserve
selected input records byte for byte, in input order.

A choice question includes NONE so that no candidate needs to win by default. Choice scores
rank candidates relative to their options. A separate yes/no fit score gates the result at
`-t` (default 0.5). TypeSafe uses Noul for yes/no; classifier translates it to binary Choice.
Their calibration is not assumed interchangeable, and a score does not authorize an action.

`is` has an unsure band (`--band`, default 0.15): yes at or above threshold plus band, no below
threshold minus band, unsure between. `filter` asks a three-way Choice (the record says the
statement holds, says it does not hold, does not say) and its rule is one-sided with a fixed
0.15 band: yes at P(holds) ≥ threshold + 0.15, no at P(does not hold) ≥ the same mark, unsure
otherwise; nothing is compared to threshold minus 0.15. By default the mark is 0.5 + 0.15 =
0.65 on either side. Unsure records are retained unless `--strict` drops them. `filter -v`
inverts yes/no selection and still retains unsure records by default.

One `is` statement prints nothing; several print `VERDICT<TAB>STATEMENT` lines. The aggregate
exit is 0 for all yes, 1 for any no, and 3 otherwise. Oversized context abstains before inference
rather than pretending that a clipped middle was judged.

## Records and batches

`pick` and `filter` read lines, paragraphs (`--para`) or NUL-separated records (`-0`). The two
split flags conflict. Blank records are omitted; original delimiters, CRLF and non-UTF-8 bytes
remain in selected human output. Machine records carry `text` and a 1-based `ordinal`; invalid
UTF-8 adds `lossy: true`. `pick` also supplies `line` and `p`.

Identical records are judged once. `filter` maps answers back to every occurrence; `pick`
returns the first occurrence of a selected distinct record. Both limit distinct records to
20,000, with `error.kind=too_many`, exit 6 above that ceiling. Input is bounded at 64 MiB.

`filter` sends up to 60 records per request on classifier.dev and 20 on TypeSafe, in parallel
subject to `JEVIFY_CONCURRENCY`. Classifier records are judged alone. TypeSafe records share one
request state, with each question identifying its record; independence is not claimed there.
Answers flow in input order as soon as earlier records are answered. A closed stdout stops work
normally. A later backend error can leave a human-output prefix and exits with that error.

## Selection rounds

Selection uses at most two rounds of parallel Jev calls. TypeSafe windows contain 200 candidates
plus NONE; classifier windows contain 99 plus NONE. A selection window shares a character budget,
clipping each candidate to 200–2,000 characters. Finalists are selected by rank within each
window, never by comparing probabilities from separate requests. With W as the backend window,
`pick`, `pick --from` and `why` keep three per window when 3 × windows fits W, otherwise two
when 2 × windows fits W, otherwise one. All finalists enter the final comparison. The first
24 finalists can receive richer evidence; 24 is not a cap on the comparison pool.

`fill` keeps three names per window in the shortlist round; with one window, when the names
leave a branch, commit, file or dir undecided, every name with p > 0 reaches the finals with its
evidence, up to 24. It accepts F = W × floor(W / 3): 3,267 candidates keyless and 13,200 on
TypeSafe. `pick` and `pick --from` accept min(W × W, 20,000): 9,801 and
20,000. Ordered kinds retain the newest candidates and report coverage; unordered overflow is
exit 6 `too_many`. `one` accepts at most W options and returns exit 2 above that count.

`fill` and `pick --from` require fit at the threshold and a winning score at least twice the
larger of the runner-up and NONE. `one` uses the same ratio; `flag` uses a fixed 0.15 unsure
band. An unsure flag abstains, since dropping it could remove a safety option. Every marker
resolves against one snapshot, with no execution if any fails. `branch` folds local/remote
twins and uses recent commit subjects and changed paths as richer evidence.

`pick --files` reads paths from stdin, selects finalists by name, then reads eligible excerpts
for the second round. Hidden and secret-looking path components and symlink files receive no
excerpt, and a file that cannot be read is named on stderr with the reason. Paths remain
candidates; the status reports `excerpts withheld: N` for both.
`-n` requests more candidates, but ranks past the third have no reliability claim.

## The `why` prefilter

`why` takes a log on stdin and no split options. It prints numbered cause lines with context.
Blank and repeated lines are removed from selection evidence. Up to 1,500 distinct lines all
go to selection; above that it keeps neighbourhoods around error-like lines within 4,000 lines,
then fills from the tail. Context comes from the full input. Compare `data.considered` and
`data.total`; incomplete selection evidence does not establish a whole-log verdict.

## Tool routing

`route` inventories PATH commands and their one-line man-page summaries. It chooses candidates
from that inventory, then judges fit using man-page excerpts of at most 12 finalists. The result
is a tool name, summary and synopsis, or exit 3. It supplies no arguments and starts no user
command. Local inventory work can overlap the API connection prewarm.

## Cache and saved inputs

Answer cache identity includes endpoint, backend, decision-contract version, model and serialized
redacted request. Answers expire after seven days, with decision fields validated before reuse.
The cache holds answers, not input text; expiry does not reclaim files. `--no-cache` or
`JEVIFY_NO_CACHE=1` bypasses it.

Only `why` and `filter` separately save raw input before inference. Their content-addressed files
are `outputs/<blake3-16>.log` under `JEVIFY_CACHE_DIR` or the platform cache directory's `jevify`
directory. They include secrets and are never pruned. `--no-save` skips saving; a failed or
skipped save reports the reason and sets `data.complete=false`. `--no-cache` does not disable
this store. [Privacy](../../PRIVACY.md) gives the permissions and outbound withholding rules.

## Models, retries and evidence

TypeSafe defaults to `jev-1.13.0`; `--model jev-latest` opts into a moving alias. Classifier
controls its model and rejects explicit overrides. `meta.model` is one string, with multiple
answering models joined by `", "`. Calibration requires evidence for that model, backend and task.
`fill` refuses any non-Jev answering model with exit 4, `api_unavailable`, and
`answered by <model>, not Jev`. A missing model name becomes `unknown` and refuses too.
The guard also applies under `--dry-run`.

Transient 408, 429, 5xx and timeout failures can be retried up to three times. A `rate_limit_day`
429 is never retried: exit 4, `daily quota of the free backend reached`. Filter batch requests
honour numeric `Retry-After` through 60 seconds and refuse longer delays; other inference calls
cap the server delay at ten seconds. Millisecond headers take precedence; HTTP-date values are
not parsed. Every verb runs under one overall deadline, `JEVIFY_DEADLINE` seconds (600 by
default); see [configuration](configuration.md). A 413 or 422 returns exit 6,
`api_rejected_request`: narrow evidence before asking again.

`meta.requests` counts inference POST attempts, including failures and retries, excluding health
and prewarm GETs. Unknown token usage is `null`. Cost estimates cover input tokens at the
configured price, not a billing receipt. [Robot mode](../ROBOT_MODE.md) defines the counters.

## Numbers

Measurements below are bounded by their date, inputs and backend; they do not validate every
verb or the classifier translation. Inputs and measurement scripts live in
[benchmarks](../../benchmarks/README.md) and [evals](../../evals/).

Latency measured 2026-09-19 on a typical macOS dev machine (Apple M4 Pro, 24 GB, macOS 26.6.2),
consumer Wi-Fi, TypeSafe `jev-1.13.0`, `hyperfine --warmup 1 --runs 15`. An empty HTTPS round trip
took about 240 ms. Cold means `JEVIFY_NO_CACHE=1`; warm means an answer-cache hit.

| Task | p50 | p95 | n | failed |
|:---|---:|---:|---:|---:|
| `pick` cold, 924 lines from `/usr/bin` | 739 ms | 804 ms | 15 | 0 |
| `pick` warm | 6 ms | 10 ms | 15 | 0 |
| `is` cold, same 924 lines | 454 ms | 488 ms | 15 | 0 |
| `why` cold, 12-line failing build | 638 ms | 732 ms | 15 | 0 |
| tool routing, without argument selection | 1,823 ms | 2,007 ms | 15 | 0 |
| `rg -c compress`, same 924 lines | 3 ms | 4 ms | 15 | 0 |

The `rg` measurement is below hyperfine's 5 ms floor and illustrates the cost of literal search.
Input-token estimates at $0.042/Mtok in these measurements: `why`, 2 requests, 1,436 tokens,
$0.00006; `is` on a six-line mail, 1 request, 346 tokens, $0.000015; `pick` over five names,
1 request, 487 tokens, $0.00002.

Routing accuracy measured 2026-09-19 on TypeSafe `jev-1.13.0`, release build, empty cache,
threshold 0.5, frozen inventory `evals/inventory.json` of 1,693 tools. BM25 ranks the same names
and man-page summaries. The routing and root-cause evaluation requests together cost $0.43
(routing $0.42, root cause $0.01).

| Routing set | n | jevify top-1 | BM25 top-1 | abstained | errors |
|:---|---:|---:|---:|---:|---:|
| author-written, routable | 39 | 36 | 9 | 0 | 0 |
| NL2Bash held-out | 120 | 36 | 4 | 66 | 0 |

Among ten requests labelled unanswerable by installed tools, nine abstain; translation to French
routes to `spit`, whose man page describes LLM translation, and counts as a miss. NL2Bash mostly
describes pipelines around `find`: 54 of 66 abstentions and 12 of 18 wrong routes name `find`
as gold. Forty requests name a utility explicitly; 16 of the 36 hits are among those forty.

Reliability among 93 accepted routes only, on the two routable sets:

| Fit bin | Routes | Correct | Accuracy |
|:---|---:|---:|---:|
| 0.4–0.6 (observed fits at least 0.5) | 17 | 12 | 0.71 |
| 0.6–0.8 | 34 | 23 | 0.68 |
| 0.8–1.0 | 42 | 37 | 0.88 |

These bins do not establish calibration below the threshold, on another backend, or for another
verb. A 0.6 threshold excludes 17 accepted routes, including 12 correct ones. Twenty of 169 routing
decisions lie within 0.06 of the threshold, the measured uncached probability jitter for identical
requests on this model. A changed model or prompt needs new evidence.

Root-cause accuracy, measured 2026-09-22 at 0.8.1 on TypeSafe `jev-1.13.0`: twenty-one real CI
logs, 136–300 lines each, hand-labelled cause ranges (`evals/why/corpus.jsonl`); `jevify why -n 3`.
Eighteen cases decided, three abstained (exit 3), zero errors. Baselines use the first or last
line matching the same error-signal regex.

| Method | hit@1 | hit@3 |
|:---|---:|---:|
| jevify | 19/21 | 20/21 |
| first signal match | 4/21 | 10/21 |
| last signal match | 1/21 | 3/21 |

The scorer (`scripts/eval_why.py`) counts a pointed line inside the labelled range whether the
verb decided or abstained, so 19/21 and 20/21 are where the pointed line fell over all 21 cases,
not the accuracy of the decided answers. Every log contains a failure, so for a caller reading
the exit code the three abstentions are misses and at most 18 of 21 are right; the hit@1 over
the 18 decided cases alone was not recorded in this run. `data.any` has minimum 0.17
and median 0.77; the abstentions score 0.17, 0.43 and 0.46. Failures include a data race, an
assertion described as “still exists”, and a lint finding. A quoted failure or library frame can
also distract selection from the cause. These counts quantify the limits of the measured task.

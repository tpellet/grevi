# How it works

## Every answer is something that already exists

grevi never writes text of its own. Every token it prints comes from your stdin, a tool on your PATH, that tool's man page, or your own request.

- `pick` returns one of the lines you gave it.
- `why` returns a line of the log.
- `run` returns the name of a tool that is installed, and flags that are in its man page.
- `add` stages changes that are in your working tree.
- `sort` returns a folder that exists.

A flag that is not in the man page cannot appear, and a tool that is not installed cannot be proposed. This is why you can use the output with `$( )` and `&&`: grevi wrote nothing, so there is nothing to sanitize.

## Two kinds of question, and NONE

Jev, TypeSafe's model, answers two kinds of question, and grevi keeps them apart.

**"Which one?"** is a choice over a list of options plus NONE. grevi adds NONE to every list, so the model can say that no line matches, no tool fits, no folder is right. A candidate only has to beat NONE to be returned. If NONE wins, grevi exits 3. The probabilities of a choice are relative to the list, so a "which one" answer is reliable at the top only. That is why the tournament (below) takes 3 per window, and why `-n` past 3 lists candidates without a ranking claim.

**"Is it?"** is an absolute yes/no with a calibrated probability. grevi asks it in five places:

- `is`: your condition.
- `why`: "does this log hold a failure at all" (`data.any`).
- `run`: "is this command a correct, direct way to do the request" (`data.fit`).
- `add`: "is this hunk about the topic".
- `sort`: the placement confidence.

Every prompt asks what a thing is: "is this command a correct, direct way to do the request". During development, the other form, "would running it accomplish the request", answered in a narrow band on almost any input. The descriptive form is the one the 0.5 default was measured on.

## One threshold

`-t` (default 0.5, env `GREVI_THRESHOLD`) gates the absolute answers only. TypeSafe documents that the two kinds of question are not on one scale, so the threshold never touches a "which one" answer. Exit 3 therefore means one of two things: nothing beat NONE, or the yes/no answer fell under the threshold.

`is` adds a dead band around the threshold, `--band` (default 0.15): yes at or above 0.65, no below 0.35, unsure in between. It is the only verb with one. The reason is measured: without the cache, identical requests move `p` by up to 0.06 between runs on `jev-1.13.0`, so a decision within 0.06 of the threshold can flip on a re-run. The other verbs report `p` and leave the margin to you.

The 0.5 default is backed by the reliability table in [README, Numbers](../../README.md#numbers): over the routes `run` acted on, fits of 0.8 and above were right 37 times in 42, and the bin just above the threshold was right 12 times in 17. The same section says what the data does not show: fits below 0.5 were not scored.

## Order is canonical

Option order is part of the request and inside the cache key. In measurements during development it never flipped a top answer, but it moved an uncertain probability by up to 0.23. So every list reaches the model in a fixed order: stdin order for `pick`, prefilter order for `why`, the inventory sorted by name for `run`, man-page order for flags, and sorted file and folder names for `sort` and for the file names `run` may append. Identical items would split the probability of a choice between them. So `pick` sends each distinct non-blank line once, and `why` drops any line already seen. The first occurrence keeps its line number.

## The tournament

The API takes at most 255 options per question. Past that, `pick` and `why` run a tournament: windows of 200 items plus NONE, 3 finalists per window (at most 24 finalists in all), then one finals round. A window's items share a 60,000-character budget, each item clipped to 200–2,000 characters, which keeps a request under the model's 32k-token state limit for typical text. The windows of a round are requested in parallel (`GREVI_CONCURRENCY`, default 8).

Every verb finishes in at most 2 rounds of requests, except `run`, which takes 3: route (which tool), fit (is the command a correct, direct way), arguments (which flags, which files). `--no-args` stops after fit.

## The `why` prefilter

A CI log can run to thousands of lines, most of them noise. Before anything is sent, `why` drops blank and repeated lines. If 1,500 or fewer distinct lines remain, they all go. Otherwise it keeps neighbourhoods of five lines around every line that matches an error-like pattern (`error`, `failed`, `panic`, `not found`, `traceback`, `denied`, `exit code N`, `assert`, and so on). It takes them from the top of the log first, because the root cause is usually the first error. Then it fills the rest of a 4,000-line budget from the tail. Context (`-C`) is read from the full log afterwards, so a kept line still shows its real neighbours.

With `why -- <cmd>`, grevi runs the command via argv, never a shell, with stdout and stderr on one pipe so the lines interleave as they would on a terminal. A daemon the command leaves behind (a build server, a watcher) can keep the pipe open after the command exits. grevi waits one second for it, then uses what it captured.

## `run`: inventory, man pages, files

`run` builds an inventory of the tools on your PATH with the one-line summary from each man page, and caches it. Names without a man page are kept. Each of the three steps reads something different:

- Route chooses among those names and summaries.
- Fit reads the man-page excerpts of at most 12 finalists.
- Arguments picks from the chosen tool's option list, and from those file names in the current directory that contain a word of your request.

Nothing else about your files, environment or history is sent ([PRIVACY.md](../../PRIVACY.md)).

`run` is the one verb that opens its connection to the API before its local work. It spends about a second reading the inventory, and the TLS handshake runs during that second. Measured on `run cold full`, that overlap is worth about 200 ms at p50. The other verbs have no local work to overlap and do not prewarm. The A/B runs are in [benchmarks/README.md](../../benchmarks/README.md).

## The cache

Answers are cached on disk (`GREVI_CACHE_DIR`, default the platform cache directory) for 7 days, keyed by the blake3 hash of the serialized request (model, prompt and the options in order). The cache holds answers (option ids and probabilities). It does not hold your text. A repeated question is answered in milliseconds (`pick warm`, 6 ms at p50) and costs nothing. `meta.cache_hits` counts the hits. `--no-cache` or `GREVI_NO_CACHE=1` bypasses it.

## The pinned model

The default model is `jev-1.13.0`, the release the 0.5 threshold was calibrated on. TypeSafe's `jev-latest` alias moves with each release, so the same input would start answering differently without any change in grevi. `--model jev-latest` (or `GREVI_MODEL`) is allowed and documented as moving. When grevi moves its default, the threshold evidence moves with it.

## Retries and limits

grevi retries a request up to 3 times on 408, 429, 5xx and timeouts. It honours the server's `retry-after` (or `retry-after-ms`) header in place of its own backoff, capped at ten seconds. When retries run out it exits 4. A 413 or 422 means the API refused the request itself, usually because dense text (hashes, paths, JSON, CJK) tokenized past the budget. grevi reports that as an input error, `api_rejected_request`, exit 6. The fix is to filter the input first. TypeSafe's rate limits are 1,200 requests per minute and 250k tokens per second.

## Redaction

Before any text leaves the machine, grevi masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`, JWTs) as `[REDACTED]`. It is a regex, so it is best effort: do not pipe secrets into grevi.

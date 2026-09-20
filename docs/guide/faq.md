# FAQ

## Do I need an API key?

No. Without a key, jevify asks [classifier.dev](https://classifier.dev). It runs the same model, Jev, and serves it free, with no account. That is what `cargo install jevify && jevify health` does on a fresh machine. Set `TYPESAFE_API_KEY` (or `TYPESAFE_API_KEY_FILE`) and jevify uses TypeSafe on your own quota instead. `JEVIFY_BACKEND=typesafe|classifier` forces a backend. `meta.backend` and `jevify health` say which one answered.

Every verb, probability and exit code works the same way on both, and the two backends score the same on the routing and root-cause evals. The free service has tighter limits. A question there takes at most 100 options, so jevify ranks in windows of 99 plus "nothing fits". An input takes 32,000 characters, and a request takes 20 questions. It is rate-limited per IP, at 3,000 classifications a minute. Details in [Configuration](configuration.md#backends).

## What does a call cost?

On classifier.dev, nothing. It is free, so `meta.input_tokens` and `meta.cost_usd` are `0` there. On TypeSafe, jevify computes the cost from the request's input tokens at `JEVIFY_PRICE_PER_MTOK` (default 0.042 $/Mtok) and reports it in `meta.cost_usd`. `-v` prints it on stderr. Three calls from the session behind the README excerpts: `why` on a 12-line build log, 2 requests, 1,436 tokens, $0.00006; `is` on a 6-line mail, 1 request, 346 tokens, $0.000015; `pick` over 5 file names, 1 request, 487 tokens, $0.00002. The two accuracy eval scripts behind the README's tables cost $0.43 in API requests together. Requests are billed to your own TypeSafe key.

A repeated identical question hits the on-disk cache for 7 days and costs nothing.

## What leaves my machine?

Only what the verb needs, and only to the active backend's API: classifier.dev without a key, TypeSafe with one. [PRIVACY.md](../../PRIVACY.md) lists it verb by verb. In short:

- `pick` sends your description and the stdin lines.
- `why` sends the filtered log.
- `is` sends your condition and the text.
- `run` sends your request, tool names with one-line summaries, man-page excerpts of the finalists, and those file names in the current directory that contain a word of your request. It never sends file contents.

Obvious secrets are masked before sending (best effort). The cache holds answers, not your text, and never crosses backends. jevify never prints or logs your key. Each service's own data handling: https://docs.typesafe.ai/legal and https://classifier.dev/privacy. On classifier.dev the requests are anonymous, but they go to a third party you did not sign up with. If that matters for your text, use a TypeSafe key or `JEVIFY_BACKEND=typesafe`.

## Why not just use an LLM shell?

Because jevify does a narrower job and can show its numbers. It never invents a flag: every flag comes from the man page of a tool that is installed. It abstains with a probability instead of guessing. Its latency is measured per verb and its cost per call. It runs only what exists on your PATH, via argv, and never the tools on its never-execute list. What an LLM does better is composing a long, exact command from scratch. jevify finds one tool and its flags, and it does not write pipelines. The measured failure modes are in [README, What it is bad at](../../README.md#what-it-is-bad-at).

## Why does it exit 3?

Exit 3 is "nothing fits" or "unsure", and it is an answer. For `pick`, `run` and `sort`, no option beat NONE. For `why`, no line looked like a failure. The usual cause is that stderr was not piped: use `2>&1` or `jevify why -- <cmd>`. For `is`, the probability landed in the unsure band around the threshold. Scripts branch on it (`case $? in 0) ...;; 1) ...;; 3) ...;; esac`), and agents escalate or ask. It is a separate code so that "no" (exit 1) and "I cannot tell" never get confused.

## Why is the model pinned?

The 0.5 threshold was calibrated on `jev-1.13.0`, and the reliability table in the README is evidence for that model only. TypeSafe's `jev-latest` alias moves with each release, so the same input would start answering differently without any change in jevify. `--model jev-latest` is allowed and documented as moving.

## Does it work offline?

No. jevify is a client of a hosted Jev API, either TypeSafe's or the free one at classifier.dev. Without a network it cannot answer a new question. Cached answers replay offline, but a new question needs the API.

## Does it work in languages other than English?

English works best. Non-Latin scripts tokenize denser than the ~4 characters per token jevify budgets for, so long CJK input can be refused by the API (exit 6, `api_rejected_request`); filter or shorten it first.

## Can I trust `p`?

As a probability, yes. Over many calls, an answer at 0.8 was right about 80% of the time, and the measured table is in [README, Numbers](../../README.md#numbers). As a fixed number, no. Without the cache, the same request moves `p` by up to 0.06 between runs, so a decision within 0.06 of the threshold can flip. Raise `-t` for costly actions. `is --band` widens the unsure zone.

## Can `run` delete something?

Not on its own. `run` executes only after a confirmation on the terminal or `--yes`, via argv, never a shell. `rm`, `dd`, `mkfs*`, `sudo`, `kill`, `shutdown` and the rest of the never-execute list, plus the shell wrappers and script interpreters, are shown and never run, whatever the confidence. The list is by tool name, so `chmod -R` is not on it: read the command before you say yes. In machine mode nothing runs without `--exec --yes`.

## Is `is` a safe filter for untrusted text?

No. The model reads input as data but is not hardened against instructions embedded in it. Use `is` and `pick` on text you control; do not use them as security gates.

## Why is `run` slow?

`run cold full` is about 1.9 s at p50. It makes three rounds of requests over the inventory of every tool on your PATH, and it renders man pages. Route-only (`--no-args`) saves about 100 ms, so most of the time goes to routing. `is` is one request and sits near the network round trip, about 450 ms. A cache hit takes a few milliseconds. The table with conditions is in [README, Numbers](../../README.md#numbers).

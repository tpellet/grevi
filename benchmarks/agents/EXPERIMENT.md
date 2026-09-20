# Does jevify improve agent decisions?

Experiment protocol · 2026-09-20 · Bead `hunch-oki`

**Status: design, not a completed A/B result.** The existing [spot checks](README.md) are
exploratory. The separate [e2e audit](E2E_AUDIT.md) checks operational behavior; passing it
does not establish an accuracy, token, cost, or speed advantage. This protocol covers all
ten current commands, with causal comparisons for the six semantic verbs and integration
comparisons for the four supporting commands. Run the pilot before funding a confirmatory
experiment. Preserve losses and inconclusive results as carefully as wins.

### Execution stages for the hardening effort

The first execution panel is [tasks.json](tasks.json): 30 original fictional cases, five
per semantic endpoint, with artifact or exact structured-answer checks. The panel spans
30 named diagnostic scenarios. Those names do **not** establish independently sampled
production families: all cases share one author and a small collection of task formats.
It is development evidence, not the independent pilot or sealed confirmation described
below. Repetitions estimate run variability; they do not enlarge the independent sample.

Before outcomes are examined, the execution sequence is:

The checked-in runner is freeze-only until an independently enforced OS boundary passes the
containment canaries below. Its `--execute` mode fails closed. A Codex workspace permission
profile did not isolate reads in two live canaries: both agents read a sibling sentinel, and
both still received host skill metadata. Those attempts remain infrastructure evidence and
cannot enter an A/B result.

1. Validate every grader with a correct result and a deliberately incorrect result; have
   a separate reviewer inspect prompts and gold. Freeze rendered input, taskset, skill,
   harness and binary hashes. Verify that agents cannot read gold, other attempts, or
   service credentials, and that the control cannot invoke jevify.
2. Run a six-task, two-arm infrastructure smoke on Astra low. Keep these attempts even if
   the harness needs repair; distinguish infrastructure failures from model decisions.
3. When isolation and accounting pass, run all 30 cases in fresh paired sessions on
   Astra low and Sol medium, one repetition each: 120 episodes. Balance order within
   endpoint; record size and task-format strata. This estimates diagnostic workload
   effects, adoption and resource requirements only. Exact resolved model identifiers
   and unavailable usage/cost fields remain visible in the manifest and report.
4. Repeat the same 120 assignments once to measure variability if the first tranche
   completes without a harness-invalidating defect. Do not choose only winning tasks
   for repetition. A changed harness or product creates a new versioned cohort.
5. Use observed operational failures and costs to revise the implementation plan. Broader
   effectiveness claims still require the independent source-family pilot and powered
   sealed evaluation specified below. Do not extrapolate a percentage saving from this
   synthetic panel into a sales claim.

Utility endpoints receive a separate live contract matrix. Their integration correctness
and elapsed times do not substitute for a randomized onboarding comparison. Classifier
availability failures remain deployment failures in that matrix; they are not labels of
semantic correctness. The main diagnostic A/B uses the reachable TypeSafe backend with
the service credential confined to a host-side jevify process.

## 1. Questions and claims

The primary question is: **for a fresh agent given a real task, does offering jevify improve
verified completion, or reduce resources while preserving completion quality?**

Three distinct quantities matter:

1. **Product effect:** agent with jevify available versus the same agent without it, including
   learning the skill, deciding whether to use it, verification, fallback, and mistakes.
2. **Semantic contribution:** Jev versus a strong local selector on identical candidates,
   excerpts, output schema, and execution policy. This isolates selection from packaging.
3. **Operational reliability:** whether a command honors its contract and produces the
   intended actual effect. Good pointers are insufficient if execution or undo fails.

Report results per endpoint, agent model, backend, and workload. An overall mean is secondary;
equal endpoint weights and any deployment-weighted mean must be frozen before test labels
are opened. No claim about all agents from one model, all workloads from one corpus, or Jev
alone from a bundled skill/tool comparison.

## 2. What the current evidence supports

Inspected source baseline: `f2fe1a3d27a81bbd9f50d4e2611dca0df4c69954`, with concurrent
uncommitted work. This is an inspection reference, **not an immutable experiment build**.
Freeze an exact source snapshot and executable hash before running the experiment.

| Observation | Interpretation and required correction |
|---|---|
| Five one-task comparisons in the agent README; the jevify arm was forbidden to read the inputs directly | Promising mechanism examples, but the constrained treatment and single trials do not estimate ordinary adoption benefit. Allow both agents full normal tools and multiple attempts. |
| Three agents per arm in the `run` pilot; no treated agent invoked jevify | This measures offering the integration on familiar tools, including overhead. It does not estimate the effect of invoking `run`. Keep familiar-tool tasks and add genuinely unfamiliar inventories. |
| One successful `add` skill demonstration | Shows a possible workflow, not a comparative success rate. Noninteractive patch construction is a valid plain-agent baseline. |
| Twenty `why` logs were normalized and cut to at most 300 lines near a known failure | Existing labels are useful development cases; they cannot validate full-log retrieval or no-failure abstention. Retain raw independent logs for heldout tests. |
| Existing `run` argument scorer checks tool/flag membership | It does not prove correct values, operand order, absence of harmful extras, or successful final artifacts. Execute safe tasks and inspect their outcomes. |
| Existing route reliability table only includes accepted routes | This is conditional precision, not full-population calibration. Collect pre-gate scores, negative cases, abstentions, and coverage. |

Sources: [agent spot checks](README.md), [log provenance](../../evals/why/README.md),
[route evaluator](../../scripts/eval_run.py), [log evaluator](../../scripts/eval_why.py).
Their published numbers were not rerun as part of designing this protocol.

The Jev context notes motivated tests of literal wording, distractors, omission, and score
stability. Vendor documentation also reports framing and long-context weaknesses. Treat
calibration as a hypothesis to validate per signal/domain/backend, not a guarantee inherited
from the API. Choice probabilities, Noul fit, and the API's derived `confidence` are different
quantities. [TypeSafe jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13),
[confidence](https://docs.typesafe.ai/confidence).

## 3. Arms and causal design

### Main A/B: ordinary agent use

| Arm | Available tools and instructions |
|---|---|
| A: competent incumbent | Normal shell, file reading, `rg`, `find`, `man`, `apropos`, Git and task-specific installed tools. No jevify or substitute hosted semantic service. |
| B: jevify available | Everything in A plus the frozen jevify skill and CLI. Agent chooses whether and when to use jevify, may inspect originals, reject its answer, and fall back. |

Common task prompt: “Complete the task in TASK.md. Use the available tools. Verify the
requested result. If the evidence is insufficient, say so. Follow the task's action
permissions. Return the required result and evidence.” The common prompt gives no routing
hint and no endpoint-specific solution. The arm-specific suffix says either “jevify is
unavailable” or identifies the frozen skill and permits jevify. Skill-reading tokens and
latency belong to B. Do not pad A to hide deployment overhead.

Choose two agent model configurations before the pilot: the intended production model and
a less expensive deployed alternative. Record exact provider snapshot, reasoning setting,
sampling settings, context window, system/tool prompts, harness version and retry policy.
Report each separately. Do not compare A on a stronger model with B on a weaker one in the
primary experiment; that is a separate substitution study. Replicate promising results in
a second harness before making harness-independent claims.

For each task × model × repetition, create two fresh sessions with identical starting
artifacts and permissions. Randomize AB versus BA in time blocks, balance each endpoint and
input-size stratum, and record the random seed. Use independent agent randomness; a shared
numeric seed is not proof of identical random streams after paths diverge. Never let a
session see another arm's transcript, labels, caches, or previous attempts.
Enforce arm-specific tool availability in the harness and execution environment, not
only in the prompt. Give agents access only to task inputs and permitted tool/docs surfaces;
keep graders, gold, service credentials and other episodes outside that visibility.
Record any crossover or evaluator-access attempt as a protocol violation without silently
dropping the episode. Evaluation tools authenticate server-side; credentials never enter
the agent transcript.

A task family is the independent sampling unit: one source repository/incident, document
collection, patch family, or tool suite. Paraphrases and reruns remain in that family. Build
fresh independent fixture repositories with `git init` for staging tasks, never clones or
worktrees of this shared checkout. Every arm gets a new writable data directory; never
reset or delete prior attempts. Keep immutable manifests and hashes of initial artifacts.

The synthetic panel's offline gold audit accepts all 30 intended outcomes and rejects a
plausible wrong outcome for each task. Its grader compares output bytes exactly, checks JSON
types recursively, records file modes and empty directories, rejects unresolved or executable
index entries, and requires the fixture commit to remain HEAD. Episode caches, configuration,
temporary files and retained patch helpers have named `.agent-*` directories outside the
task-artifact comparison; files elsewhere remain graded side effects. This validation does
not satisfy the independent-source or two-annotator requirements for the pilot.

Initial pilot budgets: 20 minutes, 100 host tool invocations, and 100,000 cumulative agent
output/reasoning tokens per episode, subject also to the provider context limit. Count
hidden reasoning only when reported. Record which ceiling ended each run. Use the same
limits in both arms, including repair attempts. Revise limits only from the pilot, then
freeze them. A timeout or exhausted budget is an unsuccessful episode, with all costs
retained. A secondary fixed-dollar/fixed-time frontier may answer deployment-budget
questions; do not replace the primary comparison with its best-looking point.

**Intention to treat:** include every assigned B episode, even if jevify is never called.
Report invocation, acceptance, override and fallback rates. Comparing only B's jevify users
against A selects harder or easier tasks after treatment and is not a causal estimate.

### Controlled mechanism comparisons

Run these on a separate development/pilot panel before locking any confirmatory claims:

- **Local selector:** same code-produced inventory, excerpts, candidates, schema and
  action checks as jevify, replacing Jev with endpoint-appropriate deterministic rules or
  tuned BM25. Tune on development/calibration only. Include abstention, not forced top-1.
- **Generative selector:** the agent model selects bounded IDs over the same evidence and
  schema, including `none`. Count its calls, invalid IDs, repairs, tokens and latency.
- **Jev selector:** jevify's actual selection/gating on those same inputs.
- **Oracle pointer:** supply a gold pointer where one exists to estimate downstream
  headroom. Label this an unattainable diagnostic ceiling, never a competitor.

For downstream attribution, randomize the local versus Jev selector behind an identically
described tool. Use the same response keys, confidence policy, excerpts and action permissions;
record any unavoidable differences. Equal confidence policy means the same predefined
error/abstention loss and calibration procedure, not the same numeric cutoff on unrelated
BM25 and Noul scales. Fix each selector's cutoff using calibration data only. If that
adapter is not implemented, component results
support selector quality only, not a causal downstream advantage. Also run a distinct full
pipeline panel: equal candidates isolate the selector but conceal jevify retrieval failures.

Forced jevify invocation, route-only versus full `run`, JSON versus JSONL/TOON, alternative
thresholds, and default versus shortened skill text are diagnostic ablations. They are not
the main A/B and must not be pooled into it.

## 4. Endpoint tasks, gold, and baselines

All rows collect completion, cost, tokens, wall time and tool/error metrics from section 6.
“Accuracy” always names its unit and denominator. An exit code alone never establishes
semantic correctness.

| Endpoint | Representative task and independent gold | Strong baseline; endpoint metrics |
|---|---|---|
| `pick` | Select the relevant real branch, issue, record or line from a frozen inventory. Gold is a set of valid IDs plus `none`, independently labelled against the request. Include multiple valid matches. | Agent `rg`/read; BM25 on the same lines. Top-1 and recall@k on answerable tasks, exact top-k/set policy, accepted precision, coverage, no-match false-positive rate, and downstream success using the pointer. Returned text/line must exist verbatim. |
| `why` | Diagnose complete public CI logs and, in a separate repair cohort, fix/retry a reproducible failing task. Gold distinguishes causal diagnostic spans from symptoms, multiple independent causes, and successful logs. | Agent search/read with unrestricted follow-up; tuned regex plus context and BM25. Causal hit@1/@3, explanation correctness, successful repair/re-run, and false alarms on successful logs. Judge all valid causal spans, not only the first error header. Test stdin and `why -- <cmd>`. |
| `run` | Achieve a verifiable filesystem/data transformation using installed tools. Separate familiar tools, obscure public tools, and heldout internal-style suites with real functional behavior and man pages. | Agent `which`/`man`/`apropos`/`--help`; BM25 inventory routing. Correct tool-set membership, executable argv correctness, required values/operands, unwanted effects, final artifact checks, refusal when no tool fits, and agent repair cost. Score `--no-args`, full proposal, and permitted execution separately. |
| `is` | Apply a literal, independently labelled decision rule to licensed documents or consented/redacted tickets; distinguish true, false and genuinely insufficient evidence. | Agent reading; tuned lexical rule and structured model boolean. Confusion matrix including abstain, sensitivity/specificity, balanced accuracy, accepted precision/coverage, and downstream correct routing. Never equate false with API failure or abstention. |
| `add` | Stage exactly the requested topic in a fixture containing unrelated work, same-file distinct hunks, pre-staged work and mixed-topic hunks. Gold specifies allowed patch content and preserved index/worktree bytes. | Agent Git diff plus constructed patch/`git apply --cached`; no interactive-only strawman. Hunk precision/recall, exact staged patch, unrelated content staged, preserved pre-existing index and worktree, repair cost. Mixed-topic indivisible hunks may require splitting via fallback or abstaining; staging unrelated lines fails. |
| `sort` | Place documents into an independently specified existing taxonomy, then verify applied moves and undo. Gold allows multiple valid folders and `none`; ambiguous taxonomy cases require review. | Agent reading plus `file`/metadata/filename rules; lexical content matching. File-level placement, exact whole-directory result, rejected placements, collision handling, content hashes, preserved files and undo restoration. Test proposal and apply separately. |
| `capabilities` | Agent discovers flags, limits and the machine envelope before solving an unfamiliar invocation. Gold is actual parser/contract behavior. | `--help`/subcommand help and available docs. First valid invocation, documentation tokens/time, invalid-call rate and subsequent task success. Schema and exit checks are deterministic; no Jev accuracy claim. |
| `robot-docs` | Agent learns a command, handles a nonzero exit or discovers privacy behavior; all supported topics plus unknown topic. | Same information in static CLI/docs. Discovery success, error recovery, token/time overhead, factual agreement with current behavior. Compare endpoints individually and as the shipped onboarding bundle. |
| `health` | Agent diagnoses reachable service, configuration/auth failure and unavailable service, then chooses a correct recovery or stops. | Documented manual checks. Correct diagnosis/recovery, false-ready rate, elapsed time, service calls, secret-free errors. Natural service observations and injected local faults are reported separately. |
| `init` | Produce and load Bash/Zsh integration in an isolated shell, then verify argument forwarding of the alias against direct invocation. | Direct jevify invocation or documented manual alias. Output correctness, shell validity, setup tokens/time and equivalent argv/result. Never source integration into the evaluator's own shell profile. |

Help/version, `--robot`, formats and invalid flags are cross-cutting interface tests.
Auxiliary-command adoption is a separate randomized onboarding panel with jevify's semantic
tool availability held constant; otherwise it confounds discovering jevify with Jev quality.
Use fresh sessions per documentation condition. Utility timings and error rates are reported
for every endpoint, but are exploratory unless separately powered and preregistered.

### Gold creation and representativeness

Use independent source families for development, calibration, pilot and sealed test. No
log from the same CI incident, near-duplicate ticket, patch template, or renamed tool suite
may cross splits. Published jevify evals, examples and audit probes are development-only.
Record source version, license/consent, sampling frame, inclusion/exclusion decisions,
checksums, task origin and transformations. Preserve full logs, not only failure windows.

Two annotators label ambiguous semantic cases without seeing arm outputs; a third resolves
disagreements. Freeze acceptable answer sets, forbidden side effects, and a reason rubric.
Report agreement and adjudication counts. An LLM can suggest labels but cannot be the sole
gold or judge of its own answer. Prefer executable artifact/state checks. Blind transcript
judges to arm names and token counts; audit a random 10% plus all grader disagreements.
Repair a demonstrably broken grader symmetrically, retain its prior version, and rerun
grading for both arms. Never narrow acceptable answers after seeing a competitor succeed.

Maintain two panels: (a) an approximately representative public/consented workload, with
frozen sampling weights, and (b) deliberately balanced stress tests. Do not advertise
the stress panel's prevalence as real usage. Authored internal-style tools test controlled
novelty; report them as synthetic suites and require real unfamiliar-tool replication
before an enterprise claim. Both arms can inspect their real man/help documentation.

For each semantic endpoint include answerable, no-valid-answer, and ambiguous cases;
lexically easy and semantic-only matches; small and large inputs; redundant and competing
options; distractor/injection text; and irrelevant-context growth. Keep the base question
answerable in perturbation pairs and label them separately when it is not.

## 5. Stress cases tied to current code

These are coverage requirements, not assertions that the implementation passes them.

| Surface | Required boundary/perturbation checks |
|---|---|
| Choice/window routing | Candidate counts 0, 1, 98, 99, 100, 199, 200, 201, 254, 255, 256 and 2,000; backends have different limits. TypeSafe windows are 200; classifier windows 99 plus NONE. Probe raw API limits separately from CLI batching. |
| Tournament loss | Gold in early/late windows; hard distractors concentrated versus spread; top-3 per-window pruning; 24-finalist cap (`pick`/`why`), 12-finalist `run` cap. Record candidate recall at each stage; cross-window Choice scores need not be comparable. |
| `why` filtering | 1,499/1,500/1,501 unique lines; 4,000-line retention budget; causal evidence before the tail with no signal keyword; repeated diagnostics, ANSI/Unicode, mixed stdout/stderr, successful child and failed child. |
| Text clipping | `is` around 96,000 characters (TypeSafe) and 30,000 (classifier), with decisive evidence in head/middle/tail. Tournament excerpts around their dynamic cap, long Unicode lines, redaction-sensitive evidence. Mark gold lost by preprocessing separately from Jev mistakes. |
| `add` | 19/20/21 hunks; long 3,000-character-clipped hunks; large aggregate state; non-repository, binary/rename/untracked inputs, pre-staged changes, subdirectory invocation, patch apply failure and partial outcomes. |
| `sort` | 9/10/11 files; 98/99/100/199/200/201 destinations; depth two/three, name-only binary input, unavailable PDF extractor, content after the first excerpt, collisions and malformed/duplicate undo data. Classifier's label limit may conflict with the 200-folder cap; verify live. |
| `run` | Correct tool absent/present in inventory, missing/stale man pages, flags absent from parsed options, `<VALUE>`, spaces/metacharacters, option values and order, refused execution, child failure. Routing has two dependent Jev rounds; argument proposal can add another. Measure instead of repeating a global two-round claim. |
| Interface | Every applicable exit 0/1/2/3/4/5/6/7/130; one valid envelope for machine modes; stdout/stderr separation; zero side effects on dry-run; cache miss/hit/stale model; unknown backend/model and service rejection. |

Recompute limits from the frozen build. Controlled HTTP faults exercise contracts with
test servers, not live Jev accuracy; keep them labelled as fault-injection tests. Do not
intentionally exhaust a public service or execute dangerous commands to obtain coverage.

## 6. Measurement contract

### Episode accounting

The primary unit is a completed or terminated task episode, not one model response or
one jevify call. Include planning, skill discovery, tool definitions, all model turns,
verification, fallback, retries and repair. Exclude offline gold creation and grading from
deployment cost but report evaluation expense separately.

| Metric | Definition |
|---|---|
| Verified success | All gold obligations and action constraints satisfied within budget; boolean per episode. Partial credit is secondary. Wrong action cannot be repaired into an unreported clean success: also record any violation. |
| Decision quality | Correct/incorrect/abstain/error counts per eligible decision; report answerable and unanswerable denominators separately. Never score missing output as abstain. |
| Agent tokens | Sum provider-reported input, output, cache-read, cache-write and reasoning fields using each provider's non-overlap mapping. Report missing components as unknown, not zero. Repeated context counts again when billed; cache categories must not be double-counted. |
| Jev tokens and total cost | Separate Jev input usage from agent usage. Compute dollars from dated provider pricing and cache/reasoning billing rules. Record free-backend consumer charge separately from unobserved compute usage. Cross-model raw token sums are labelled, not treated as interchangeable compute. |
| Context payload | Tool response bytes plus exact agent-tokenizer counts where available; also peak context and cumulative delivered tool tokens. This is a diagnostic, not a substitute for billed tokens. |
| Wall time | Monotonic start before onboarding to final result/budget termination. Report mean, median, p95 and time-to-verified-success by the fixed deadline. Preserve failures at the deadline; don't report only successful fast runs. |
| Agent tool calls | Host tool invocations and failed invocations. Separately count shell subprocesses, jevify invocations, semantic questions, HTTP POST attempts, retries, prewarm GETs, and parallel/dependent request rounds. One shell loop may contain 24 jevify calls. |
| Tool errors | Rates per attempted relevant invocation AND episodes with any error; stable categories below. Report absolute counts and denominators; raw nonzero-exit rate is not semantic error rate. |
| Recovery | Extra agent turns/tokens/time after an incorrect result, service failure, or abstention; number of fallbacks and eventual success. Label cause from evidence, not a guessed chain of thought. |
| Side effects | Expected versus observed file/index changes, hash preservation, unintended actions, no-op correctness and restoration. Actual readback overrides `executed`, `staged` or `applied` flags. |

Errors are separated into agent syntax/schema/unknown-tool errors; nonexistent path/invalid
argument errors; wrong semantic decision with exit 0; intended domain negative (such as
`is` exit 1); appropriate abstention; child failure; input limit/rejection; authentication;
transport/rate-limit/service failure; malformed envelope/panic; and unintended mutation.
`why` wrapping a known failing child and `rg` finding no matches are not automatically
tool failures. Freeze expected-exit semantics in each task's checker.

Never subtract a guessed constant system-prompt cost. Report full deployment totals and,
separately, measured task-variable tokens. Total cost per verified success is
`sum(cost of ALL episodes) / number of verified successes`; undefined with zero successes.
Mean cost conditional on success is descriptive only because treatment changes who succeeds.

### Current telemetry limitations: implementation prerequisite

In [client.rs](../../src/jev/client.rs), `Stats.requests` increments on HTTP 200 only;
transport failures, non-200 attempts and prewarm GETs are absent. Usage is added after
successful parsing. The classifier adapter supplies no usage, yielding zero input tokens;
that is **unavailable usage**, not zero semantic work. `meta.cost_usd` uses the configured
price estimate, not a billing receipt. `meta.elapsed_ms` is a CLI timing, not episode time.
Only the last TypeSafe request ID is exposed; intermediate stages/attempts are not traced.

Before confirmatory runs, add optional structured, redacted stage/attempt telemetry or an
equivalent instrumented transport that actually forwards live calls. Required events:
inventory/filter/window/finalist/gate/args/action; candidate counts and evidence hashes;
actual gate score/type/threshold/NONE mass; request start/end/status/retry; bytes, usage and
usage availability; cache events; action readback. Capture scores for candidates that were
rejected too. Do not infer those events from final-envelope counts. Validate counter
conservation against controlled retry/cache traces and a small live run.

Proposed harness records (new schema to implement, not fields already present in jevify):

```text
manifest: protocol_hash, source_hash, binary_sha256, fixture_manifest_hash,
          split, seed, assignment_schedule, agent_config, backend, requested_model,
          cache_policy, platform, tool_inventory_hash, prices, budgets, grader_hash
episode:  task_id, family_id, endpoint, stratum, pair_id, replicate, arm,
          start, duration_ms, terminal_reason, verified_success, violations,
          agent_usage{reported_fields, normalized_counts, missing_fields},
          jev_usage{tokens, available}, total_usd, cost_available,
          host_tool_calls, jevify_calls, http_attempts, retries, cache_hits,
          semantic_errors, invocation_errors, infra_errors, fallback_count,
          before_hashes, after_hashes, trace_ref, grade_ref
event:    episode_id, event_id, parent_id, stage, monotonic_time,
          candidate_count, retained_gold_count, score_kind, scores, threshold,
          model_returned, status, usage_available, elapsed_ms
```

Gold-related trace fields are joined **after** execution by the evaluator, never sent to
the agent. Store task data and sanitized traces under run-specific immutable paths;
append attempts, do not overwrite the existing evaluators' fixed `evals/out/*.json` files.
No credentials, headers, private documents or personal local paths in public artifacts.
Audit IDs can remain private; public manifests reference sanitized trace hashes.

### Latency and caching controls

Primary cold-decision panel: no semantic answer-cache hits; fixed, already-built inventory
snapshot in both arms. Count skill onboarding in every fresh episode. Second panel measures
fresh installation/inventory discovery; third uses realistic repeated workloads and warm
caches. Never combine these into one speed claim. Record agent prefix-cache billing too.
Warm jevify results prove replay speed, not fresh Jev inference quality.

Freeze backend explicitly; TypeSafe and classifier.dev are separate treatments because
limits, batching, billing visibility and availability differ. Record requested AND returned
model; quarantine a version-change block and repeat both arms on a stable version. Do not
mix backend results under one `jev-latest` label. Balance traffic/time blocks without
running A/B simultaneously against shared rate limits. Start with one episode at a time;
increase concurrency only after a harness contention check. Record OS, CPU, network region,
inventory state and provider incidents. Final portability evidence includes macOS and Linux.
Record service quota consumption separately from token billing when exposed: classification
units may differ from request counts. Existing `hunch-glz` reports keyless quota/retry stalls;
its proposed cause remains a hypothesis until service accounting confirms it. Preflight
remaining quota and never run the main experiment into a known exhausted service budget.

## 7. Calibration and abstention analysis

First evaluate the **shipped** threshold/band unchanged. A separately labelled calibrated
variant can be tuned on calibration families only, then frozen. A single global threshold
is an explicit product hypothesis: compare its heldout risk/coverage by endpoint with
endpoint-specific calibrated variants; do not silently change the product during a test.

Map each score to its actual proposition before scoring calibration:

- `is.p`: truth of the literal condition (binary-labelled subset); ambiguity is separate.
- `add` hunk Noul: membership in the requested topic.
- `pick`/`why` `any`: existence of an adequate candidate **in the retained evidence**, not
  correctness of the selected line. Choice mass is relative to that candidate set.
- `run.fit`: whether the proposed command is a direct fit; it does not certify argv.
- `sort` existence Noul and folder Choice: distinct signals; final `moves[].p` alone does
  not expose the full decision gate.

Report Brier score and reliability plots with bin counts and intervals for each labelled
binary proposition; ECE is secondary and specifies binning. For top-choice correctness,
assess the actual selection probability on heldout labelled candidate sets separately.
Never interpret a high existence probability as confidence that the returned action is safe.

Plot selective error versus coverage across frozen threshold sweeps on test for description,
without choosing a new winner on test. Report correct abstention on unanswerable inputs,
false abstention on answerable inputs, and inappropriate high-confidence acceptance.
When jevify abstains, count the fallback cost and eventual episode outcome in A/B results.
Test equivalent paraphrases, option permutations and independent uncached repeats; report
decision flips near threshold and by backend. Perturbation variants stay clustered with
their source task and do not multiply the independent sample size.

## 8. Statistical plan and sample sizes

For endpoint `e`, task `i`, repetition `r`, let `Y` be verified success. Estimate
`Delta_e = mean_i(mean_r(Y_B - Y_A))`. For resources report absolute paired differences and
the ratio of arm means, `R = mean(resource_B) / mean(resource_A)`; savings are `1 - R`.
Do not average percentage savings over tiny or zero denominators. Within each endpoint,
use frozen representative-panel sampling weights; equal task weights if none are justified.

Use a paired cluster bootstrap resampling source families within strata, keeping all arms,
models and repetitions together (10,000 resamples, fixed analysis seed). Report ordinary
95% intervals descriptively; use family-adjusted one-sided bounds for headline claims.
For few independent clusters, label inference exploratory and show family-level results.
Exact McNemar is a useful binary paired check only for independent single-trial tasks;
it cannot treat repeated seeds or related logs as independent observations. Repeated
trials measure stability, not extra independent tasks. These choices follow the paired
and clustered evaluation principles in [Adding Error Bars to Evals](https://www.anthropic.com/research/statistical-approach-to-model-evals).

Select one primary agent model/backend/cache condition. The confirmatory headline family
has **12 hypotheses: quality and efficiency for each of six semantic endpoints**. Use
Bonferroni one-sided alpha `0.05/12` for each headline. An efficiency headline is an
intersection test: every quality/cost/token/time guard below must pass at that level.
Other models, backends, strata and metrics get complete descriptive intervals, not extra
unadjusted discovery claims. A new confirmatory family requires a new preregistration.

Suggested practical decision rules, to lock before confirmatory execution:

| Claim | Required evidence |
|---|---|
| Better decisions | Adjusted lower bound on `Delta_e` > 0, point improvement >= 3 percentage points, no observed critical action violation; publish all resource changes even if more expensive. |
| Saves agent tokens without material quality loss | Adjusted lower bound on `Delta_e` > -2 percentage points; adjusted upper bounds: agent-token ratio < 0.85, total-dollar ratio < 1.00, and p95 episode-time ratio < 1.10; no observed critical violation. Missing cost/usage disqualifies the corresponding claim. |
| Faster / fewer errors / cheaper | Exploratory unless separately registered and powered; a token win does not imply any of these. Publish estimates and intervals with their own denominators. |
| No practically meaningful difference | An equivalence interval contained inside a prespecified tolerance, not merely a nonsignificant superiority test. |
| Inconclusive | Bounds cross a decision threshold, sample size is inadequate, telemetry is incomplete or gold is unreliable. Do not rename it “no effect.” |

The 2-point noninferiority margin is a proposed product tradeoff, not a fact about acceptable
risk. In `add`, `sort`, and executed `run`, separately count any unrelated staging, wrong
move or forbidden effect. Zero observed critical failures is necessary for an efficiency
claim, but does not prove zero risk. For independent trials with zero failures the exact
one-sided 95% upper bound is `1 - 0.05^(1/n)` (about `3/n`); correlated repetitions cannot
be substituted for `n`. A bound below 1% needs at least 299 independent opportunities.

### Staged allocation

1. **Development/calibration:** curate separate families, validate gold, check telemetry,
   tune local baselines and optional thresholds. Existing evals belong here. No test access.
2. **Pilot:** 30 fresh task families per semantic endpoint × 2 repetitions × 2 arms ×
   2 agent configurations = **1,440 agent episodes**. This is a planning allocation, not
   a powered claim. Start with a 5-family-per-endpoint harness smoke subset, retain its
   records, and stop for instrumentation/grader failures. Utility panels are additional.
3. **Power and budget review:** estimate paired discordance, between-family variance,
   cost distribution and p95 stability from the pilot. Simulate the full paired,
   clustered, multiplicity-adjusted decision rule. Choose fixed test N for at least 80%
   power on the smallest worthwhile effect; estimate both expected and upper-tail spend.
   Never silently reduce N and keep the same claim strength when the budget is too small.
4. **Sealed confirmation:** fresh families, locked manifest/grader/prompts/thresholds and
   schedule, one primary model/backend; two repetitions initially unless pilot variance
   justifies a different frozen allocation. Replications use new families when feasible.

Scale warning: with paired discordance `q=0.20` and true success improvement `d=0.05`, a
simple independent-pair normal approximation gives
`n ~= (z_(1-alpha) + z_0.80)^2 * (q - d^2) / d^2`.
At one-sided alpha 0.025 this is about **620 independent pairs**; at `0.05/12`, about
**960**. Establishing a 2-point noninferiority margin when true difference is zero can
need about **6,060** independent pairs at the adjusted level. These are illustrative
planning calculations, not a guarantee; clustering and joint efficiency guards can
increase the requirement. More repetitions of 30 tasks cannot substitute for this breadth.

Pilot cost estimate: `1,440 * measured mean episode cost`, plus component panels, utility
tests and grading. Confirmatory per-endpoint cost: `2 arms * N families * repetitions *
mean episode cost`; use actual backend/harness price records. No large paid experiment
is launched by this design session. If funding only supports the pilot, publish a pilot.

Do not peek for significance and stop at a win. Fixed N, no outcome-dependent extensions;
an extension is a new experiment. Pause for a predefined critical unintended mutation,
grader leak, corrupt trace, changed model, or service failure rate >10% in the last 20
episodes. Preserve failures and costs. A harness infrastructure failure before treatment
starts may be excluded under the frozen rule; after assignment retain the failed row in
intention-to-treat analysis and record any replacement pair separately. Report sensitivity
to service-outage blocks without deleting them from the primary product result.

## 9. Strengths, weaknesses and hardening priorities

Current strengths are structural capabilities or preliminary signals, not proven A/B wins:
bounded pointers into real inputs; explicit abstention; normal Unix composition; structured
envelopes; deterministic action checks; semantic selection where lexical overlap is weak.
The strongest hypotheses are long-log diagnosis, literal document triage and unfamiliar
tool discovery. Small exact lookups and familiar tools are essential negative controls.

| Failure signature | Diagnosis to establish | Next improvement to test |
|---|---|---|
| Gold never reaches Jev | Retrieval/filter/clipping ceiling | Better candidate coverage or explicit incomplete-evidence signal; measure added input cost. |
| Correct candidate lost between windows | Tournament/pruning error | Candidate diversity or alternative shortlist rule within the frozen latency budget. |
| Correct tool, wrong final artifact | Argument/execution gap | Value/operand handling and verification; do not improve only route hit rate. |
| Cheap wrong answer accepted confidently | Calibration/framing failure | Per-signal reliability and fallback policy on new calibration data. |
| Accurate jevify, worse episode cost/time | Integration overhead or excessive invocation | Shorter onboarding, targeted use, batch opportunities; retain full A/B accounting. |
| No invocation on unfamiliar tasks | Discovery/usability failure | Inspect traces, then randomize integration changes; don't discard non-users. |
| Zero tokens/requests despite network trouble | Observability failure | Attempt-level counters and nullable usage before any economic claim. |
| Backend-only limit failures | Contract/adapter mismatch | Match batching and size limits to each service; preserve exact regression case. |

The accompanying audit found a concrete preprocessing failure: redaction maps both sides
of `token_expiry_seconds=3600` → `token_expiry_seconds=7200` to the same masked text,
and live `add` abstained. Preserve this failed case (`hunch-2v2`). An ablation may compare
the original and transformed **synthetic non-secret** evidence to localize the loss;
never disable redaction on real private data to improve a benchmark score.

Prioritize using expected deployment frequency × severity × measured repair cost, with
security/data-integrity defects first. Use mechanism traces to assign retrieval, selector,
gate, execution, integration, grader or infrastructure ownership. After a fix, keep the
old case as a regression and collect a fresh heldout family before claiming improvement.

## 10. Delivery and experiment execution gates

The design deliverable is this protocol plus the separate bounded e2e report. Implementation
and confirmatory results remain separate Beads with concrete acceptance criteria:

1. **Observability (`hunch-hjp`):** immutable manifests/episode ledger and complete attempt/stage counters;
   usage missingness explicit; tests for retries, cache hits, truncation and interrupted runs.
2. **Gold and tasks (`hunch-ric`):** independent family splits for every semantic endpoint, blinded
   adjudication, real outcome checkers, representative and stress panels, utility cases.
3. **Harness/pilot (`hunch-evl`):** randomized A/B, normal fallback, scoped permissions, budgets,
   isolated data, counter-conservation checks, repeated trials and complete failure ledger.
4. **Analysis/confirmation (`hunch-s18`):** fixed primary model/backend, pricing and power simulation,
   sealed test, paired cluster intervals and multiplicity-aware claim table.
5. **Hardening:** redaction-induced evidence loss (`hunch-2v2`), classifier sort limits
   (`hunch-v3r`), full run rounds (`hunch-u07`), and maintained live coverage (`hunch-3te`).
   Execution blocks only
   the affected confirmatory endpoint while independent corpus/instrumentation work proceeds.

Dependency graph: `hunch-oki` → observability + gold → harness/pilot → confirmation.
The existing `hunch-zss` supplies the unfamiliar-tool cohort; `hunch-eil` tracks the
argument-quality weakness, and `hunch-glz` tracks keyless quota/retry behavior.
The bounded audit is `hunch-q3i`, separate from fixing its
findings or claiming exhaustive coverage.

A completed report has one row per endpoint/model/backend/panel: independent families,
episodes, verified successes, paired quality difference with interval, agent token and
dollar ratios with intervals, p50/p95 time, tool calls, invocation and semantic error
rates, abstention coverage, uptake/fallback, critical effects, missing telemetry, and
claim status. Include all rows, examples of both wins and losses, frozen manifest hashes
and a reproducible analysis command. Do not publish only the best endpoint or seed.

Agent evaluations should grade observed environment outcomes and retain multiple trials
and traces, as described in [Anthropic's agent evaluation guide](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents).
TypeSafe's [skill-suggestion experiment](https://docs.typesafe.ai/cookbooks/skill_suggestion)
is a useful vendor example of a bounded routing intervention, not evidence that jevify
achieves its gains. All jevify performance claims must come from jevify's own frozen runs.

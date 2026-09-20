# E2E audit — 2026-09-20

Status: **local gates passed; bounded TypeSafe live probes exercised all six semantic verbs; classifier probes were mostly unavailable**. This audit does not establish an accuracy, token-saving, or latency advantage over an agent baseline or constitute comprehensive E2E coverage.

## Scope and fingerprint

An Astra agent at low reasoning effort applied the E2E Pipeline Validator skill. It inspected the ten CLI commands, ran the complete default suite, exercised the actual binary without network access, and ran the real local inventory test. No production code or tests were changed. Semantic selections in the default tests use Wiremock/FakeJev and therefore are contract evidence, not evidence that Jev makes correct decisions.

- Starting HEAD: `ee9eaed7194f6e6098435c71f7d4673c0918a7cf`; working tree was clean at the initial inspection. The lead subsequently changed tracker/documentation files concurrently. No isolated checkout was made.
- Binary: debug build of version 0.3.0; SHA-256 `346f0c202b91a8c6dbe18378884a01ba88551e87e7494e6ca195df9efc4b8544`.
- Toolchain: rustc 1.93.1, cargo 1.93.1; macOS. These are debug-build observations, not release performance measurements.

## Executed checks

| Check | Result | What it establishes |
|---|---|---|
| `cargo fmt --check` | Pass | Formatting |
| `cargo check --all-targets` | Pass | All targets compile |
| `cargo clippy --all-targets -- -D warnings` | Pass | Current lint gate |
| `cargo test -- --test-threads=1` | 94 passed, 4 ignored | Unit and mocked contract suite; no live semantic evidence |
| `cargo test --test live inventory_finds_many_tools_here -- --ignored --test-threads=1 --nocapture` | 1 passed | Actual PATH/man inventory, required documented-tool count and expected installed commands |
| Direct no-network binary probes | 15/15 returned parseable envelopes matching process exit | Local dispatch, validation, missing-key behavior |
| `ubs benchmarks/agents/E2E_AUDIT.md` | Exit 3: no supported language | Markdown is not scanned; this is **not a UBS pass** |

The quality gate and local inventory test ran outside the sandbox, as the project requires. The inventory contained 1,886 tools; one cold load took 2.669 seconds and one cached load 2.471 milliseconds. This single paired observation is informational, not a stable speedup estimate.

The direct probes explicitly selected the TypeSafe backend with both credential variables absent and caching disabled. No service request occurred. A newly created empty temporary directory served as the working directory for filesystem/Git probes.

| Command | Direct probe | Exit and observed behavior |
|---|---|---|
| `pick` | Empty input; two-line input | 5, `missing_api_key` in both cases; authentication precedes input validation |
| `why` | Empty stdin | 5, `missing_api_key` |
| `run` | Dry-run, route-only, benign request | 5, `missing_api_key`; no child executed |
| `is` | Empty stdin; band 0.6 | 5, `missing_api_key`; 2, `usage`, respectively |
| `add` | Dry-run outside a Git repository | 6, `input`; shared repository index untouched |
| `sort` | Empty directory without destinations | 6, `input`; no moves |
| `capabilities` | JSON | 0, complete envelope |
| `robot-docs` | Guide; unknown topic | 0; 2, `usage` |
| `health` | Missing TypeSafe key | 5, `missing_api_key` |
| `init` | zsh; invalid shell | 0, script returned; 2, `usage`; script not installed |
| Global flags | Threshold 1.1 | 2, `usage` |

The default suite additionally passed mocked semantic abstention and probability-band tests, multi-window selection, backend translation, retry/cache behavior, actual safe child execution with mocked selection, selected-hunk staging in temporary repositories, and sort dry-run/apply/undo in temporary directories. Those tests validate plumbing and effects conditional on mocked decisions. They do not validate real model quality.

## Scoped live continuation

Automatic approval review rejected the standard key-backed live-suite command before it ran. Its stated reason was that transmission of local or fixture-derived data to the external TypeSafe service lacked an established, explicitly authorized payload/destination scope. The rejection was not bypassed through classifier.dev or another execution path. No credential contents were read or printed by the agent.

The lead subsequently established a narrower scope, and automatic review approved separate calls whose justifications named the exact fictional inputs and service destinations, excluding private files and machine inventory. This continuation used real TypeSafe/classifier services with synthetic fixtures; it did not rerun the denied broad command. The three network-dependent ignored tests remain **NOT RUN** as tests, although narrower overlapping behaviors were exercised below. Production-corpus data still requires its own scope; artificial fixtures cannot establish real-world accuracy.

The shared checkout had meanwhile advanced to HEAD `063069d21e8024fbbb3f6734c7a77739c82ace88`. A preserved copy of the already-built debug binary reported v0.3.1 and SHA-256 `a46d94774efc549fc4c601ad74a3b87532b9a108b492f6b2f5fdcb3869c15e52`. All continuation CLI probes used that copy, so subsequent builds could not change it. The earlier gate results apply to the earlier snapshot; they were not silently reassigned to v0.3.1.

The lead independently reran formatting, all-target compilation, Clippy and the default
suite on the v0.3.1 checkout at 04:15 UTC: all passed, with 94 tests passing and four
live tests ignored. The lead also verified the four retained-record hashes below.
Changed-file UBS again returned exit 3 because Markdown and JSONL are unsupported;
document links, endpoint coverage and whitespace were checked separately.

TypeSafe probes returned model `jev-1.13.0`, default threshold 0.5, with caching disabled. Candidate inventory for `run` explicitly contained only `pwd` and `wc` and their descriptions; public man-page text enriched those choices. No private installed-command inventory was submitted. Each timing below is one observation of a fresh CLI process, not an estimated population mean.

| TypeSafe probe | Result | Wall time / reported input tokens / successful POSTs |
|---|---|---|
| `pick` invoice among three filenames | Correct original line 2, p=1, any=.97, exit 0 | 1,019 ms / 422 / 1 |
| `pick` nonexistent railway timetable in same candidates | Correct abstention, any=.11, exit 3 | 325 ms / 421 / 1 |
| `why` three-line fictional compiler failure | Correct undefined-name cause at line 2, exit 0 | 538 ms / 1,102 / 2 |
| `is` explicit refund request | Yes, p=.99, exit 0 | 386 ms / 324 / 1 |
| `is` replacement request explicitly rejecting refund | No, p=.02, exit 1 | 382 ms / 324 / 1 |
| `add` token-expiry increase, dry-run and apply | Miss: abstained p=.22 both times; neither hunk staged | 373/382 ms / 451 each / 1 each |
| `sort` invoice and poem into Finance/Poetry, dry-run | Both correct, original files intact | 367 ms / 623 / 1 |
| `sort --apply` then `--undo` | Both files moved correctly; undo restored both original byte strings | 565/20 ms / 623/0 / 1/0 |
| `run --no-args --dry-run` | `pwd`, fit=.98, executed=false | 875 ms / 1,051 / 2 |
| `run --no-args --exec --yes` | Actually executed `pwd`, child exit 0 | 739 ms / 1,051 / 2 |
| Full `run --dry-run` requesting physical working directory | `pwd -P`, fit=.54, flag p=.98; three dependent rounds | 1,011 ms / 1,505 / 3 |
| `health` | Reachable, exit 0; requests counter nevertheless zero | 307 ms / 0 / 0 reported |

Two follow-up diagnostics were retained separately from these original cases. A literal token-expiry topic also abstained (p=.05, 380 ms, 464 tokens); it does not replace the original failure. A separate color-change control successfully selected and actually staged `color=blue` to `color=green` (p=.98), leaving the unrelated logging hunk unstaged (p=.03). `git show :style.txt` showed `color=green`, `git show :logging.txt` remained `level=info`, and the remaining unstaged file was only `logging.txt`. That control reported 449 tokens, one POST and 435 ms internal elapsed time.

Classifier requests used the same fictional inputs and explicit backend URL. Of 13 cases, 12 returned exit 4, `api_unavailable`, at approximately 30.2 seconds with `model=null` and `requests=0`. These are availability failures, not semantic-quality judgments; the model never supplied an evaluable decision. Sort/apply and add failures left fixture content and indexes unchanged. The 99-folder probe was unavailable; the 100-folder probe returned exit 6, `api_rejected_request`, in 134 ms. The latter corroborates the schema-mismatch concern, but the normalized record did not retain the server's detailed rejection reason, and no successful 99-folder comparison exists. No further outage probes were launched after this finite batch completed. Existing issue `hunch-glz` tracks classifier retry/quota stalls; this audit does not identify the same underlying cause.

There were 30 continuation CLI invocations: 17 TypeSafe-configured (including local undo and the public-log probe below) and 13 classifier-configured. These are CLI counts, not physical HTTP-attempt counts. The lead's independently executed pick probe is outside this audit count.

### Real public-log check

One additional real-input check used the existing 300-line `evals/why/cargo-01.log`, whose adjacent expectation records the MIT-licensed [Tokio CI job](https://github.com/tokio-rs/tokio/actions/runs/35300150683/job/105460901444) and a gold cause span of lines 288–295. The fixture README documents normalization, redaction and windowing; this is a previously collected development case, not a fresh holdout or a representative full-job sample. The source/license attribution was read from those repository records, not independently re-fetched during this audit.

The preserved v0.3.1 binary ran `--json why` with that fixture on stdin, TypeSafe explicitly selected and caching disabled. It returned exit 0 and correctly selected line 288, the unresolved `IntoRawHandle` import diagnostic, with p=.88 and any=.81. It considered 241 distinct lines of 300 total, reported three successful POSTs and 10,576 input tokens, and took 718 ms wall time. The three POSTs are consistent with two first-round windows and one final round; they do not imply three dependent rounds here. This single hit establishes a working real-input path, not an accuracy rate or comparison against other tools.

Input SHA-256: `7495ed9b830b683d59d453cf111b29bbd07e2d4995f0d760614954415aebc143`; expectation SHA-256: `476008febb3c69196e248c743d09693963a397559f9fb4a718e19f71bd6b42e3`. Complete CLI stdout was retained separately as `public-cargo-01.stdout.json`, SHA-256 `919cda441ae592f1c19a237955d9d2ed250e7d6e3f545c066c2aedf48a46a471`, alongside `public-cargo-01.stderr.txt`. These later artifacts are outside the earlier normalized-record manifest.

### Reproduction inputs and retained evidence

The literal semantic inputs were `notes.txt\ninvoice-2026-03.pdf\ncat.jpg\n` with intents `the invoice from March` and `a railway timetable`; `Compiling demo\nerror[E0425]: cannot find value \`missing_name\` in this scope\nerror: could not compile demo\n` for `why`; and `Please refund my order. I do not want a replacement.\n` / `Please send a replacement. I do not want a refund.\n` with condition `the customer requests a refund` for `is`.

The add fixture staged a baseline `settings.txt` containing `token_expiry_seconds=3600\n` and `style.txt` containing `color=blue\n`, then changed their working copies to 7200 and green. The original topic was `extend authentication token expiry`; the follow-up literal topic was `increase token_expiry_seconds from 3600 to 7200`. The independent control used `style.txt` blue→green and `logging.txt` info→debug with topic `change the color from blue to green`. These were fresh `git init` repositories, not clones or worktrees; no commit was needed because the baseline was in each fixture's index.

Sort inputs were exactly `Invoice for office supplies. Total 42 dollars.` in `march.txt` and `A poem: roses are red, violets are blue.` in `poem.txt`, with Finance and Poetry destinations. The restored SHA-256 values matched hashes independently computed from those expected literal bytes: invoice `577cb97f188040ee0060a21f84f736e4999c970ece6f7b023ae3fcd53a5cc1d4`; poem `57442c3ec31b8725d444890dd5e1e28cc678ffc0e26bc7ce7da7cc286d65dc3d`. Boundary fixtures contained the same invoice text with Finance and numbered generic folders (99/100 total).

The explicit run inventory was `[{"name":"pwd","summary":"print working directory"},{"name":"wc","summary":"count lines, words, and bytes"}]`. Route-only intent was `print the current working directory`; full proposal intent was `print the physical current working directory`. Every CLI probe used `--json`, an explicit backend/base URL, and `JEVIFY_NO_CACHE=1`; only the TypeSafe CLI read the configured key file. Dry-run/apply/undo flags are shown in the result table.

The scoped scratch directory retains these normalized records and SHA-256 hashes:

| Artifact | SHA-256 |
|---|---|
| `semantic-probes.json` (10 records) | `7936a7b32498e3b44f8d058125374e948754688ef88133606a4130a76d74b2f2` |
| `effects-probes.json` (15 records) | `c2a995f20a0a6a539392ef3974b7d2bfed5986f4a8fde4a98d969a604d732fba` |
| `diagnostic-probes.json` (3 records) | `5be83c82e98f08f0402140f8d890fead312a8160354e56e036670dc753b589a5` |
| `manifest.json` (binary, records, inventory and redaction-probe hashes) | `759790a9ba4ad8b6355572f1beee2094019795c45fbe9b2f8f0ca65413b595c7` |

These records preserve selected output fields rather than complete stdout envelopes/HTTP traces. The separate color control and index/hash checks are in the session evidence and described above; they are not included in those 28 JSON records. This limitation precludes retrospective exact HTTP-attempt/retry reconstruction. The experimental harness must capture complete structured telemetry from the outset.

## Concrete findings for hardening and measurement

1. **Request accounting omits failed attempts.** Source inspection of `src/jev/client.rs::post` shows `stats.requests` increments only after an HTTP 200 body has been read. Retries, timeouts, rejected requests and authentication failures are omitted. `health` uses a separate HTTP client and does not increment this counter. Thus `meta.requests` is not total network attempts and cannot alone support tool-call-error, retry-overhead or service-call claims. Add attempt/status/error counters and distinguish logical calls, physical attempts and successful responses. Regression-test a 429 followed by 200 as two attempts and one success, and all-failure paths as nonzero attempts.

2. **Classifier sort destination limit is inconsistent.** `src/cmd/sort.rs::folders` takes up to 200 destinations and adds `NONE`; `src/jev/classifier.rs` declares a 100-label maximum. The sort path does not window destination choices, and request serialization sends every criterion. Therefore 100 destinations produce 101 labels; 200 produce 201. This is a source-confirmed schema mismatch, corroborated by the live 100-folder input rejection, with the rejection-detail limitation above. Add boundary coverage for 99, 100 and 200 destinations on each backend and preserve all candidates through an appropriate selection strategy or return an explicit supported-limit error.

3. **Full `run` requires a third dependent semantic round in an observed case.** Routing awaits shortlist windows, then finalist fit scoring in `src/cmd/run.rs::route`; afterwards argument proposal awaits another `client.ask` in `src/args.rs` when applicable. The live `pwd -P` probe confirmed three successful POSTs with only two inventory candidates. Route-only fits the two-round design; full argument proposal can exceed the stated per-verb principle. Measure route-only and argument proposal separately, and decide whether to change the implementation or the documented bound. Physical requests can exceed logical rounds through windows, backend question splitting and retries.

4. **Backend token/probability semantics differ.** Classifier translation returns default usage (`input_tokens = 0`) and maps an absolute TypeSafe Noul to relative two-label scores. Zero reported tokens means unreported backend token usage, not zero inference work. A shared numeric threshold does not demonstrate cross-backend calibration. Report unknown service tokens explicitly and evaluate each backend/model separately, including reliability curves, coverage, selective error and abstention cost.

5. **Current live tests are too narrow for a full E2E claim.** The live suite has local inventory, classifier pick, classifier health and TypeSafe routing only. It lacks live `why`, `is`, `add`, `sort`, argument correctness, mutation verification with real selections, and semantic error/limit cases. Extend the live matrix before describing every endpoint as fully tested; keep deterministic contract tests separate.

6. **Secret redaction destroys a legitimate configuration diff.** The failed token-expiry case was traced to preprocessing, not simply attributed to Jev. A standalone Rust probe linked against `jevify::input::redact` transformed `-token_expiry_seconds=3600` and `+token_expiry_seconds=7200` into `-token[REDACTED]` and `+token[REDACTED]`. The optional delimiter in the secret regex lets an identifier suffix be treated as a secret. The model consequently cannot see the change it is asked to select. Preserve this as an end-to-end failure and add regression coverage for ordinary configuration identifiers/numeric values while retaining genuine credential masking. Preprocessing ablations should distinguish lost evidence from model reasoning errors.

Follow-up beads: request accounting `hunch-hjp`; sort limits `hunch-v3r`; run round contract `hunch-u07`; live matrix `hunch-3te`; redaction evidence loss `hunch-2v2` (P1). The lead owns tracker updates and integration.

## Remaining evidence requirements

For each semantic verb, run approved real-corpus happy, ambiguity, no-match, negation, long-input, candidate-boundary, and misleading-input cases through both configured backends. Record immutable input/gold hashes, model identity, argv, exit/envelope, wall-clock duration, logical and physical request counts, bytes/token usage availability, cache state and actual effect verification. For `add`, verify exact staged patches and unchanged worktree content in a disposable non-clone repository. For `sort`, verify source/destination bytes, collisions, dry-run immutability and undo. For `run`, compare selected command and argv to executable gold and verify benign actual effects. Do not turn model mistakes into passing tests by relaxing gold after observing outputs.

This audit left an empty scratch directory (suffix `e2e-audit-9w1wol9h`), a scoped probe directory (suffix `e2e-scoped-20260920-0414`) containing the preserved binary, fictional fixtures, sanitized result records and a redaction probe, and the generated sort undo log `sort-undo-1789877697.tsv`. They were not deleted. Existing test-managed temporary directories follow the project's established test behavior. No benchmark payloads or raw service responses were added to this repository.

UBS also reported scanner scratch directories `tmp.AhB1ClsOHE` and `tmp.TCzYsE8oMb`;
no manual cleanup was performed.

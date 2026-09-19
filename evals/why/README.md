# `why` eval cases

Twenty real failing logs from public GitHub Actions runs of MIT/Apache-2.0 projects, four per
ecosystem (cargo, npm/tsc, pytest, go, docker), each hand-labelled with the root-cause line range.
`scripts/eval_why.py` runs `hunch why --json -n 3 < <id>.log` on every case and scores hit@1 and
hit@3 against `<id>.expect`, next to two regex baselines on the same files.

Collected 2026-09-19. The runs were the most recent genuine tool failures on each repository's
default or PR branches at that date (workflow cancellations, runner infrastructure faults and
obvious flakes were skipped and are not represented).

## Files

- `<id>.log` — the failing job's log, normalised (below). Between 136 and 300 lines.
- `<id>.expect` — `{ "lines": [start, end], "source": "<job URL>", "license": "<SPDX id>" }`.
  `lines` is the 1-based inclusive range in `<id>.log` that a competent engineer would point to as
  the cause: the first diagnostic that names the actual problem, through the end of its block (the
  `-->` location and the source excerpt of a rustc/tsc/ruff diagnostic; the `E` lines and the
  `file:line: AssertionError` of a pytest failure; the `--- FAIL` header and the assertion message
  of a go test). Summary lines are never labelled (`error: could not compile`, `FAILED tests/...`,
  `Process completed with exit code 1`, `ERROR: failed to solve: ...`, `make: *** Error 1`). When
  several independent errors appear, the first one's block is labelled.
- `source` is the job URL inside the run; the raw log is downloadable from there while GitHub keeps
  it (90 days from the run).

## Normalisation

Applied to the raw job log (`gh api repos/<owner>/<repo>/actions/jobs/<job>/logs`) before saving:

1. the `<job>\t<step>\t<timestamp> ` prefix GitHub puts on every line is removed;
2. ANSI escape sequences are removed (a tool writing to a pipe does not emit them either);
3. runner-only lines starting with `##[` (`##[group]`, `##[endgroup]`, `##[error]`, ...) and
   `[command]` are dropped, and the log is cut at `Post job cleanup.`. Note that GitHub's problem
   matchers rewrite some diagnostics (tsc, eslint, go compile errors, golangci-lint) as
   `##[error]<line>`, so this rule dropped them too: the cases below were chosen where the tool's own
   output survived, which biases the sample toward test failures over compile errors for go and npm;
4. runs of blank lines are collapsed to one;
5. hunch's `input::redact` regex (`src/input.rs`) is applied. It fired on 3 files, every hit a false
   positive on `token...`/`tokenizer` in test ids and model warnings, none on the labelled lines;
6. a window of at most 300 lines is kept, ending at or shortly after the failure (`window` below).
   Where the tail of the job included the checkout step's masked `token: ***` echo, the window was
   moved to exclude it.

No log contains an email address, a credential, or a home directory other than the runner's.

## Provenance

| id | repository | license | job | failure | window kept | root-cause lines |
|---|---|---|---|---|---|---|
| cargo-01 | tokio-rs/tokio | MIT | [job](https://github.com/tokio-rs/tokio/actions/runs/35300150683/job/105460901444) | `cargo doc --all-features`: `error[E0432]` unresolved import `IntoRawHandle` in windows-only `named_pipe.rs` | last 300 of 600 lines | 288–295 |
| cargo-02 | ratatui/ratatui | MIT | [job](https://github.com/ratatui/ratatui/actions/runs/33818641650/job/100856226867) | `cargo minimal-versions check`: resolver conflict on `bitflags` (^2.11 vs selected 2.9.0) | lines 259–394 of 394 | 121–131 |
| cargo-03 | sharkdp/fd | MIT OR Apache-2.0 | [job](https://github.com/sharkdp/fd/actions/runs/34986632764/job/104440291301) | `cargo clippy -D warnings`: `clippy::io_other_error` denied in `src/error.rs` | lines 172–420 of 420 | 230–241 |
| cargo-04 | nushell/nushell | MIT | [job](https://github.com/nushell/nushell/actions/runs/35194238619/job/105174299306) | `cargo test` (Windows): `assertion left == right failed` in a nu-cli completion test (backslash vs slash) | last 300 of 3,701 lines | 277–282 |
| npm-01 | vitejs/vite | MIT | [job](https://github.com/vitejs/vite/actions/runs/35400465072/job/105779041544) | `pnpm run build`: rolldown `[PARSE_ERROR] Identifier fileToUrl has already been declared` (duplicate import) | lines 130–322 of 322 | 166–175 |
| npm-02 | vitejs/vite | MIT | [job](https://github.com/vitejs/vite/actions/runs/35263531980/job/105345046146) | vitest (Windows): `ssrStacktrace.spec.ts` AssertionError, backslash vs slash in a stack trace path | last 300 of 473 lines | 275–284 |
| npm-03 | colinhacks/zod | MIT | [job](https://github.com/colinhacks/zod/actions/runs/35144047901/job/104955470997) | vitest typecheck under TypeScript 5.5: `TypeCheckError: Cannot find name 'Temporal'` (first of 7) | lines 1150–1449 of 1,987 | 251–257 |
| npm-04 | vitest-dev/vitest | MIT | [job](https://github.com/vitest-dev/vitest/actions/runs/32539425329/job/96946672485) | vitest browser test: inline snapshot `timeout hooks 1` mismatched (Expected/Received diff) | last 300 of 1,172 lines | 258–269 |
| pytest-01 | pydantic/pydantic | MIT | [job](https://github.com/pydantic/pydantic/actions/runs/35411255497/job/105811190024) | pytest: `PydanticDeprecatedSince20` raised while importing `pydantic.v1._hypothesis_plugin` | last 300 of 775 lines | 201–208 |
| pytest-02 | pytest-dev/pytest | MIT | [job](https://github.com/pytest-dev/pytest/actions/runs/35243408403/job/105278391752) | pytest: `test_do_not_collect_symlink_siblings` — `assert_outcomes(passed=1)` got `passed: 0` (first of 8) | lines 500–799 of 3,506 | 257–265 |
| pytest-03 | psf/black | MIT | [job](https://github.com/psf/black/actions/runs/34987498453/job/104617444176) | pytest (win-arm64, py3.15): `test_expression_diff` — `AssertionError: 123 != 0` on the `--diff` exit code | last 300 of 1,360 lines | 241–244 |
| pytest-04 | pydantic/pydantic | MIT | [job](https://github.com/pydantic/pydantic/actions/runs/35248454086/job/105294519235) | pre-commit `ruff check`: `D200 One-line docstring should fit on one line` in `pydantic/plugin/__init__.py` | last 300 of 649 lines | 194–199 |
| go-01 | helm/helm | Apache-2.0 | [job](https://github.com/helm/helm/actions/runs/35346725778/job/105604936715) | `go test`: `--- FAIL: TestSqlUpdate`, sqlmock "could not match actual sql" | lines 90–262 of 262 | 162–168 |
| go-02 | helm/helm | Apache-2.0 | [job](https://github.com/helm/helm/actions/runs/35279589611/job/105398199049) | `go test`: `--- FAIL: TestStatusWaitMultipleNamespaces/...`, "resource ... still exists" | last 300 of 449 lines | 288–295 |
| go-03 | ollama/ollama | MIT | [job](https://github.com/ollama/ollama/actions/runs/35277313408/job/105390981312) | `go test`: `--- FAIL: TestPullHandlerForceBypassesFitCheck`, blob requests = 2, want at most 1 | lines 1550–1849 of 2,756 | 189–190 |
| go-04 | ollama/ollama | MIT | [job](https://github.com/ollama/ollama/actions/runs/35009350621/job/104517105390) | `go test -race`: `WARNING: DATA RACE` in `TestPullModelManifestListDownloadsSelectedChildOnly` | lines 600–899 of 1,911 | 155–158 |
| docker-01 | distribution/distribution | Apache-2.0 | [job](https://github.com/distribution/distribution/actions/runs/34435460797/job/102740205237) | `docker buildx bake`: Dockerfile parse error, `COPY --from=binary /registry` with one argument | lines 590–874 of 874 | 279–283 |
| docker-02 | distribution/distribution | Apache-2.0 | [job](https://github.com/distribution/distribution/actions/runs/34444127685/job/102767511732) | `docker buildx bake`: `pull access denied` loading metadata for `docker.io/upx/upx:latest` | last 300 of 6,378 lines | 223–224 |
| docker-03 | home-assistant/core | Apache-2.0 | [job](https://github.com/home-assistant/core/actions/runs/35415573571/job/105824031008) | `docker build`, `RUN uv pip install`: no solution found, `installer>=1.0` unavailable for `pipdeptree==4.2.2` | lines 130–378 of 378 | 204–210 |
| docker-04 | moby/moby | Apache-2.0 | [job](https://github.com/moby/moby/actions/runs/35392841457/job/105755013498) | bin-image bake (linux/arm/v6), `RUN apt-get install libc6-dev:armel`: unmet dependencies, held broken packages | last 300 of 2,035 lines | 257–259 |

Licences were read from the GitHub API (`license.spdx_id`); `sharkdp/fd` reports Apache-2.0 there but
ships both `LICENSE-APACHE` and `LICENSE-MIT`, so it is recorded as the dual licence. The logs are
build output of those projects, reproduced here under their licences for evaluation only.

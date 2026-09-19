# AGENTS.md — hunch

Read `~/.claude/AGENTS.md` (Thomas's global rules) first, every session. This file adds
project rules and wins where the two conflict. Rules adapted from
Dicklesworthstone/asupersync's AGENTS.md are marked (asupersync).

## What hunch is

A fast Rust CLI that points at the right thing among real things — a line, a tool, a hunk, a
file — using TypeSafe's Jev model, with calibrated confidence and an honest "nothing fits".
- Spec: `docs/superpowers/specs/2026-09-18-hunch-v0-design.md`
- Plan: `docs/superpowers/plans/2026-09-18-hunch-v0.md` (source of truth for tasks; beads transcribe it)

Principles: point, never generate · at most two rounds of parallel Jev calls per verb · one
calibrated threshold · Unix-first human output, one machine envelope · safe by default.

## Hard rules

- **No file deletion without Thomas's written permission.** No `git reset --hard`, `git clean`,
  `rm -rf`, force-push. `dcg` blocking a command is not a puzzle to route around.
- **`main` only. No branches, no worktrees, no scratch clones, no PRs.** Commit to `main`
  directly. (asupersync)
- **Never read, print, copy or search `~/.ssh/*`.** For live calls, point hunch at the key with
  `TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key` in the command's environment — hunch reads
  it, you never do. Never echo the variable or the key. Live commands need the sandbox disabled
  because `~/.ssh` is sandbox-denied.
- **Never commit secrets**, never paste API responses that contain keys, never log the key.
- **No script-based code edits** (sed/regex rewrites of source). Edit by hand. (asupersync)
- **No file proliferation**: revise files in place; no `*_v2.rs`. (asupersync)
- **No backwards-compatibility shims**: no users yet; fix things directly.
- Treat unexplained working-tree changes as another agent's work: never stash, revert or
  overwrite them; commit only your own paths.

## Toolchain (Rust & Cargo)

- Cargo only. Edition 2024, stable toolchain (rustc ≥ 1.87 for `std::io::pipe`; local 1.93). (asupersync)
- `#![deny(unsafe_code)]` in `src/lib.rs`. Tests may use `unsafe { std::env::set_var(..) }`
  only inside `tests/` (edition 2024 marks it unsafe) and must run single-threaded.
- Dependencies: explicit versions, minimal set, prefer std. Adding a crate needs a one-line
  justification in the commit body. `toon-format` must stay `default-features = false`.
- Async: tokio current-thread runtime only; no second executor.
- Verify third-party APIs against docs.rs or the downloaded crate source, not memory.

## Quality gate (before every commit, all must pass)

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
ubs <changed files>          # exit 0 required; verify findings, fix root causes
```
A red gate and a skipped gate look the same from outside: read the output, report exactly what
ran. Never weaken a test or a gate to land a change. (asupersync)

Live tests (`tests/live.rs`, all `#[ignore]`):
```bash
TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key cargo test --test live -- --ignored --test-threads=1
```

## Testing policy

- Every module has inline `#[cfg(test)]` unit tests: happy path, edge cases (empty input,
  limits such as 200/255 options, huge input), error conditions. (asupersync)
- Every verb has a contract test on exit code and `--json` envelope using `tests/common::FakeJev`
  (wiremock). No live network in the default test run.
- CLI output is minimal, deterministic and documented; stdout carries results only. (asupersync)

## Work tracking (Beads)

- `br ready --json` → pick → `br update <id> --claim --actor <name> --json` → work →
  gate → commit → `br close <id> --reason "<what + evidence>" --json`.
- Commit subject: semantic prefix + bead ID, e.g. `feat: add hunch pick (hunch-abc)`;
  first line < 72 chars; no `Co-Authored-By`. Commits are SSH-signed; never bypass signing.
- `bv --robot-triage` / `--robot-plan` is advisory; never run bare `bv` (TUI).
- `br sync --flush-only` before committing `.beads/` changes; `br` never runs git.
- Close only verified work; append blockers and evidence with `br update --append-notes`.

## Agent-facing contract (do not break without updating capabilities + docs + tests)

Exit codes 0 ok · 1 no · 2 usage · 3 abstain · 4 unavailable · 5 auth · 6 input · 7 child
failed · 130 declined. Machine envelope: `{ok, command, version, exit_code, data, meta, error}`
with `error{kind, message, hint, example}`. Error `kind` strings are stable identifiers.
New verbs update `cli.rs`, dispatch, `capabilities()`, `docs/ROBOT_MODE.md`, README and tests.

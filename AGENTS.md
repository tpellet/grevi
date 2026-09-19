# AGENTS.md — hunch

A fast Rust CLI that points at the right thing among real things — a line, a tool, a hunk, a
file — using TypeSafe's Jev model, with calibrated confidence and an honest "nothing fits".

- Spec: `docs/superpowers/specs/2026-09-18-hunch-v0-design.md`
- Plan: `docs/superpowers/plans/2026-09-18-hunch-v0.md` (source of truth for tasks; beads transcribe it)

Principles: point, never generate · at most two rounds of parallel Jev calls per verb · one
calibrated threshold · Unix-first human output, one machine envelope · safe by default.

---

## Toolchain: Rust & Cargo

- Cargo only. Edition 2024, stable toolchain. `rust-version = "1.87"` is the floor for
  `std::io::pipe` (`why -- <cmd>`); the local toolchain is pinned to 1.93.
- `#![deny(unsafe_code)]` in `src/lib.rs`. `unsafe { std::env::set_var(..) }` only inside
  `tests/` (edition 2024 marks it unsafe).
- Async: tokio current-thread runtime only; no second executor. Blocking local work (stdin,
  inventory, `man`) goes through `tokio::task::spawn_blocking` or `std::thread::scope` so the
  runtime keeps driving in-flight HTTP.
- Dependencies: explicit versions, minimal set, prefer std. Adding a crate needs a one-line
  justification in the commit body. `toon-format` must stay `default-features = false`.
- Verify third-party APIs against docs.rs or the downloaded crate source, not memory.
- macOS and Linux only for v0.

---

## Quality Gate (before every commit, all must pass)

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
ubs <changed files>          # exit 0 required; verify findings, fix root causes
```

Run the gate and every `git commit`/`git push` with the sandbox disabled: wiremock binds
127.0.0.1 and SSH signing needs `~/.ssh`, both sandbox-denied. Never `#[ignore]`, weaken or
delete a test to get past a sandbox failure.

Live tests (`tests/live.rs`, all `#[ignore]`):

```bash
TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key cargo test --test live -- --ignored --test-threads=1
```

Without a key they print `SKIPPED: set TYPESAFE_API_KEY_FILE=...` and return; a hand-off lists
them as NOT RUN, never as passed.

---

## Testing

- Every module with pure logic has inline `#[cfg(test)]` unit tests: happy path, edge cases
  (empty input, the 200/255-option limits, huge input), error conditions. Thin orchestration
  modules are covered by the contract tests in `tests/` instead.
- Every verb has a contract test on exit code and the `--json` envelope using
  `tests/common::FakeJev` (wiremock). No live network in the default test run.
- Tests run single-threaded (`--test-threads=1`): integration tests spawn the binary with
  per-process env; one thread keeps wiremock ports and stdin handling deterministic.

---

## hunch — This Project

**`main` only.** No branches, no worktrees, no scratch clones, no PRs. One shared checkout.
Parallel agents all work in it and coordinate through Agent Mail: each registers, reserves its
exact write paths before its first edit, and posts in the bead's thread (global rule:
`~/.claude/AGENTS.md` RULE NUMBER 2). Tasks with a real dependency stay
serial. If Agent Mail is down, run serially; never fall back to a worktree. Stage and commit only
your own paths (`git commit -- <paths>`); the index is shared. Commits are SSH-signed; never bypass
signing. Commit subject: semantic prefix + bead ID, e.g. `feat: add hunch pick (hunch-abc)`.

**Unattended runs: never trigger a confirmation prompt.** Agents run while Thomas is away; a
prompt nobody answers stalls the whole run. If a destructive or mutating step seems needed
(`rm`, `git reset --hard`, `git stash`, force-push, `mv` over an existing file, anything `dcg`
blocks), skip it, record it in the bead notes or your report, and continue. Leave scratch files
in place. Never use `rm` in any form (`rm -f`, `rm -rf`), `rmdir`, `unlink`, `git rm`,
`find -delete`, `cargo clean`, `git restore`, `git worktree remove` or `git branch -D`, even on
your own scratch files: overwrite with the Write/Edit tool, write a new filename, or create a new
directory instead. Use non-interactive flags (`-y`, `--yes`) for every command. `dcg` blocking a command is not a puzzle to route around.

**Public repo: scan before every push.** `tpellet/hunch` is public; a push is publication. Before
every `git push`: `gitleaks git . --log-opts="origin/main..HEAD" --redact -v` (no leaks; the repo's
`.gitleaks.toml` applies), then read `git diff origin/main..HEAD` for keys, tokens, pasted API
responses, absolute local paths, session URLs, personal details, other people's data. On a hit:
fix forward in a new commit, never rewrite history, never force-push. Never commit communications
drafts (tweet, Show HN, announcements): they live in `~/Projects/hunch-launch/`, outside the repo,
and never go into bead fields or commit messages. User-facing docs say
`TYPESAFE_API_KEY_FILE=/path/to/key` and "a typical macOS dev machine", never the real key path
or personal tooling. Commit bodies stay technical: no `Co-Authored-By`, no `Claude-Session:`
trailer. Global rule: `~/.claude/AGENTS.md` "Public Repositories — Push With Care".

**Live API key.** Point hunch at the key with
`TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key` in the command's environment — hunch reads
it, you never do. Never echo the variable or the key. Live commands need the sandbox disabled
because `~/.ssh` is sandbox-denied.

### Agent-facing contract (do not break without updating capabilities + docs + tests)

Exit codes 0 ok · 1 no · 2 usage · 3 abstain · 4 unavailable · 5 auth · 6 input · 7 child
failed · 130 declined. Machine envelope: `{ok, command, version, exit_code, data, meta, error}`
with `error{kind, message, hint, example}`. Error `kind` strings are stable identifiers.
New verbs update `cli.rs`, dispatch, `capabilities()`, `docs/ROBOT_MODE.md`, README, a
PRIVACY.md row, and tests.

# jevify: the two sides of a command — implementation plan

This plan implements `docs/VISION.md`. The vision decides the product; this document decides the
code. Where they disagree, the vision wins and this file is corrected.

Bead IDs keep the `hunch-` prefix. Existing beads are named where they apply; a bead that does
not exist yet is written `new: <title>` and gets its ID when it is filed.

Code references (`file:line`) point at the tree at version 0.4.0.

## 1. The contract in one page

```
jevify VERB [options] ['text'…] [-- COMMAND ARGS…]
```

| Verb | Side | Reads | Starts a command | Exit codes |
|:---|:---|:---|:---|:---|
| `fill` | input | argv after `--`; stdin or a file for `-`, `one`, `flag` | yes, the only one | 0 ran ok, 7 ran and failed or died on a signal, 2–6 nothing ran (6 includes a command that cannot start) |
| `pick` | output | stdin, or `--from KIND` | no | 0 found, 3 abstain |
| `route` | output | the PATH inventory (`pick --from tool`) | no | 0 found, 3 abstain |
| `why` | output | stdin | no | 0 found, 3 abstain |
| `filter` | output | stdin | no | 0 kept some, 1 kept none, 3 every record unsure |
| `label` | output | stdin | no | 0 labelled, 3 every record unsure |
| `is` | output | stdin | no | 0 all yes, 1 one no, 3 otherwise |
| `add`, `sort` | acting verbs, unchanged | — | git apply / rename only | unchanged |

Removed, with no shim, all in Phase 1 (release 0.5.0): `why -- CMD`, the verb name `run`,
`run --yes`, `run --exec`, the `run` argument pass (`src/args.rs`, `manpage::parse_flags`).
`init` stays: its `,` alias and its `command_not_found` hook call `jevify route`, which only
prints (`src/cmd/agent.rs:188-203` call `jevify run` today).

A child killed by a signal is exit 7 with `ran, signal N` on stderr. Exit 130 keeps its meaning
(declined or interrupted inside jevify, `src/exit.rs:14`).

Marker grammar (from the shell audit, `docs/VISION.md` "The marker"):

```
marker  := "@{" KIND ":" BODY "}"
KIND    := "-" | [a-z][a-z-]*            registered kinds only
BODY    := bytes up to the first unescaped "}"; "\}" is a literal "}"
one     := "@{one:" OPT ("|" OPT)* ":" QUESTION "}"      escapes \: \| \}
flag    := "@{flag:" FLAGTEXT ":" QUESTION "}"           whole argv element; FLAGTEXT starts with "-"
escape  := "@@{"  →  literal "@{"
```

- One wrapping pair of `'…'` or `"…"` around BODY is dropped. A newline in BODY becomes a space.
- Escapes are decoded once, before the separators of `one` and `flag` are read. Substituted text
  is never lexed again, so a handle that holds `@{` stays literal.
- Exit 2, nothing runs: unknown kind (the error names the nearest kind and lists the kinds this
  build has), `@{word:` with no closing `}`, empty BODY, BODY that is not UTF-8, `flag` embedded in
  a larger argument, zero markers, a `one` with fewer than two options or a repeated option, a
  marker in `argv[0]` (the command name is always literal), more than 20 `one`/`flag` markers.
- Every exit-2 message is one line that an agent can act on in one turn: the corrected argument
  in single quotes, and `@@{` for a literal `@{word:`. A command given as one string (`argv[0]`
  holds a space) gets "pass the command as separate arguments".
- `@{` followed by anything that is not `KIND:` is literal (`@{u}`, `HEAD@{2}`, `@{1 day ago}`).
- Every other byte of every argument passes through unchanged. Argv is `OsString` end to end.
- After `--` every token belongs to the command, `--dry-run`, `--json` and a second `--` included
  (`jevify fill -- git log -- '@{file:…}'`). jevify never diagnoses or consumes them.

## 2. Architecture

New files, each with no existing home:

| File | Contents | Tests |
|:---|:---|:---|
| `src/marker.rs` | Pure lexer: `parse(&[OsString]) -> Result<Vec<Arg>, MarkerError>`; `Arg = Vec<Piece>`; `Piece = Lit(OsString) \| Marker { kind, body, span }`; `substitute`. No I/O. | inline table tests |
| `src/records.rs` | The record model shared by `@{-:…}` and every output verb, over raw bytes from the new `input::read_stdin_bytes()`. `Record { handle: OsString, evidence: String, raw: Range<usize>, ordinal: usize }` indexes one input buffer; `evidence` is the lossy, ANSI-stripped view of the record, redacted when it is sent. Splitting and reading are independent (2.3). | inline |
| `src/source.rs` | `Kind` enum and `enumerate(kind, scope) -> Listing { records, total, omitted, scope }`; evidence tiers; identity; a cap, a deadline and a collector class per kind (2.2). Enumerators run an argv, never a shell; a timed-out collector is killed and reaped. | inline + `tests/fill.rs` |
| `src/cmd/fill.rs` | Orchestration: lex → read inputs → enumerate → resolve → substitute → re-check → print or spawn. The only `Command::spawn` for a caller's command. | `tests/fill.rs`, `tests/fill_pipe.rs` |
| `src/cmd/filter.rs`, `src/cmd/label.rs` | Per-record verbs over `records.rs`; the per-record scorer lives in `filter.rs` and `label.rs` uses it. `add` keeps its own batches. | `tests/filter.rs`, `tests/label.rs` |

Changed in place:

| File | Change |
|:---|:---|
| `Cargo.toml`, `Cargo.lock` | tokio feature `signal`, rustix feature `process`, version 0.5.0 |
| `src/cli.rs` | `Fill`, `Route` (no `--yes`, no `--exec`), `Why` without `cmd`, later `Filter`, `Label`, `Pick --from`, `Is` with several statements; help text says that `-C` is a directory for `fill` and context lines for `why` (`src/cli.rs:75`) |
| `src/lib.rs` | `VERBS` (`:23`) and `command_name` (`:128`) list the verbs of the release; the clap error path reads `args_os`, stops at `--` and never panics on a non-UTF-8 argument (`:63` uses `env::args()`); machine format on a `fill` run is exit 2; the `fill` error tail (2.1 step 11); `human` written as bytes; `-v` lines carry the prefix `jevify <verb>:` |
| `src/cmd/mod.rs` | `Outcome.human: Vec<u8>` (`:15` is `String`); `Outcome.tail: Option<String>`, the last stderr line, printed by `run_cli` after the `-v` lines |
| `src/exit.rs` | one variant `JevifyError::Kinded { kind, exit, message, hint, example }` for the new stable kinds (today `Input(String)` collapses to the kind `"input"`, `:74-87`); the text of exit 7 names `fill` (`:29` names `run`) |
| `src/input.rs` | `read_stdin_bytes()` with the same 64 MiB cap and terminal check; `is_fill_status(&[u8])`; `read_stdin` drops `fill` status lines (2.1) |
| `src/output.rs` | `shell_quote(&[OsString]) -> Vec<u8>`, moved from the private `run.rs:312` `shell_display(&[String])` and written over bytes |
| `src/tournament.rs` | `window` becomes `pub(crate)` (`:55`); `decide(&Ranking) -> Decision` holds the found rule of 2.1. `rank` keeps its behaviour, so `pick` and `pick --files` do not change |
| `src/cmd/why.rs` | delete `capture` and the `cmd` argument (`:59-`); later the saved input |
| `src/cmd/run.rs` | becomes `route`: delete execution, the confirmation, the argument pass; print tool, summary, synopsis. The file is renamed only with the owner's permission (section 9) |
| `src/cmd/pick.rs` | `--from KIND`; later the record model |
| `src/cmd/is.rs` | several statements |
| `src/cmd/agent.rs` | capabilities, robot-docs, `init` |
| `src/config.rs`, `src/jev/client.rs` | Phase 0; the save directory of 2.4 |
| `tests/common/mod.rs` | `FakeJev` answers with a probability vector per question (section 5) |
| Documents | `docs/ROBOT_MODE.md`, `README.md`, every file of `docs/guide/` (32 lines show `jevify run` or `why --` today), `PRIVACY.md`, `CHANGELOG.md`, `AGENTS.md` (the `rust-version` note cites `std::io::pipe`, whose only use is `why.rs:63`; the two-round sentence names the `run` argument pass; the exit 7 text; the plan pointer), `plugins/jevify/skills/jevify/SKILL.md` (top-level `skills/` is empty), `plugins/jevify/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json` |

Dependencies. No new direct dependency. tokio's `signal` feature adds the transitive crate
`signal-hook-registry` to `Cargo.lock` (it has no entry today); the commit body carries the
one-line justification `AGENTS.md` requires: signal handling under `#![deny(unsafe_code)]` needs
a safe wrapper, and a hand-written `sigaction` handler needs `unsafe`. tokio `process` is not
needed: the child is a `std::process::Child`, waited on with `spawn_blocking`, and signalled with
`rustix::process::kill_process` (rustix feature `process`, already a dependency). The
current-thread runtime stays. JSON records use `serde_json`; NUL records need no crate.

### 2.1 Resolution algorithm (`fill`)

`W` is the window of the backend in force: 200 candidates on TypeSafe, 99 on classifier.dev
(`Backend::window`, `src/config.rs:34`).

1. **Lex and validate** the whole argv and the `fill` options before any I/O. Usage errors stop
   here; no request, no enumeration, no spawn. A machine format (`--json`, `--format`) without
   `--dry-run` is exit 2 with the corrected line. A literal `argv[0]` that is not on the PATH is
   exit 6 `command_not_found`, also before any request.
2. **Plan inputs.** `-` reads `--candidates FILE` or stdin. `one` and `flag` read
   `--context FILE` or stdin. When both families need stdin, one snapshot serves both. Files
   named by options are opened first, relative to the directory jevify was started in; then
   `-C DIR` is applied as a `chdir`, so enumerators, identity checks and the command all run in
   `DIR`, as with `git -C` and `make -C`. stdin is read once, on the blocking pool, up to 64 MiB;
   more is exit 6 `input_too_large`. A required stdin that is a terminal is exit 6
   `stdin_is_tty`. A context above the backend's evidence budget is exit 3
   `insufficient_evidence` with no request, as `is` does (`src/cmd/is.rs:21-31`).
3. **Enumerate** each distinct `(kind, scope, prefix)` once, in parallel, under the kind's
   deadline. A failed or timed-out collector is exit 6 `enumerator_failed` with a bounded tail of
   the tool's stderr; the collector is killed and reaped. A kind whose collector runs project
   code needs `--allow-collectors` (2.2); without it, exit 6 `collector_not_allowed`, and the
   message shows the piped form (`cargo test -- --list | jevify fill -- cargo test '@{-:…}'`).
   Then, per marker:
   - A candidate whose handle cannot be substituted is dropped and counted in `omitted`, never
     ranked: a handle that holds a newline, a carriage return or a NUL; a handle of a kind that
     is not UTF-8; a handle that starts with `-` when the marker opens its argument and the kind
     is not a path. After a literal prefix (`'--label=@{-:…}'`) a leading `-` is harmless and
     the handle stays.
   - Two candidates with the same handle and the same evidence are one candidate. Candidates
     with the same evidence and different handles stay separate; they split the Choice mass and
     end as `ambiguous`, and the status line says `N candidates share the same evidence`, which
     tells the caller to add `--evidence` fields.
   - Zero candidates is exit 3 `no_match` with no request. classifier.dev rejects a Choice of
     one label (`src/jev/classifier.rs:99`), and the answer is known without asking.
   - More than `W` candidates: a kind with a recency order (`branch` by committer date,
     `commit`, `pr`, `issue`, `ci-run`) keeps the newest `W` and reports `candidates 99 of 1432, newest first`. Every other kind is
     exit 6 `too_many_candidates` with no request; the message gives the count, `W`, and the two
     ways to narrow: a literal path prefix, or a narrower list piped into `'@{-:…}'`.
     `fill` never resolves through several windows: `tournament::rank` keeps three candidates per
     window and compares probabilities from different requests (`src/tournament.rs:122-127`), so
     the right candidate can be gone before the finals (hunch-zxz). That is acceptable for
     `pick`, which prints; it is not for a verb that runs.
4. **Resolve**, at most two rounds.
   - Round 1, all at the same time: each listing marker is one `tournament::window` call over
     tier-one evidence (one Choice with `NONE`, one `any` Noul, one request). All `one` and
     `flag` markers go out as **one request** over the context: one Choice (the options plus
     `NONE`) per `one`, one Noul per `flag`. 20 questions is the limit of classifier.dev per
     request (`MAX_DIMENSIONS`, `src/jev/classifier.rs:26`), hence the limit of 20 in section 1.
   - Round 2, only for a listing marker that failed the ratio below and whose kind has tier-two
     evidence: fetch tier two for the top 24 of that window (`MAX_FINALISTS`) and call `window`
     once more. All 24 come from one Choice, so their order is meaningful. `rank` is not used:
     it always runs a finals round when `finalist_text` is given (`src/tournament.rs:110-144`),
     which would cost the second round even when the first was decisive.
5. **Decide** per marker.
   - Listing kinds: found when `any ≥ threshold` and
     `p(best) ≥ 2 × max(p(second), p(NONE))`. A ratio holds for 3 options and for 200; a
     difference does not, because a Choice over 200 options spreads its mass. One candidate has
     `p(second) = 0`. Equal scores abstain. The factor 2 is a constant in `tournament.rs` and is
     reported in the status line and in `data`; it is a guard, not a calibration.
   - `one`: the same ratio over the options and `NONE`; no Noul.
   - `flag`: `is::band_verdict` with the default band (`src/cmd/is.rs:54-62`). Yes keeps the
     argument. **No or unsure leaves it out**; the status line says
     `flag --draft: unsure 0.48, left out`. A flag marker adds an optional argument, so doubt
     means the command runs as written without it. `flag` never abstains.
   - Reasons for exit 3, in `data.reason` and in the status line: `no_match` (`any` below the
     threshold, or `NONE` wins), `ambiguous` (the ratio fails after round 2),
     `insufficient_evidence` (the context is over the budget).
   - classifier.dev answers a Noul as a two-label Choice (`src/config.rs:9-10`); `any` and the
     `flag` band have a contract test on each backend.
6. **All or nothing.** Every marker is resolved before anything starts. Any abstention means
   nothing runs: exit 3, empty stdout, one line per failed marker:
   `jevify fill: not run: arg 3 branch: ambiguous; closest: tp/auth (0.41), tp/auth-v2 (0.38)`.
   This covers the caller's command only: inert collectors have already run, under `--dry-run`
   too.
7. **Substitute.** Replace each marker's span with the handle bytes. A path handle is relative to
   the prefix it was listed under, so `'src/@{file:…}'` becomes `src/cmd/add.rs`, never
   `src/src/cmd/add.rs`. When a path marker opens its argument and the handle starts with `-`, it
   gets `./`. A `flag` that is left out removes its whole argv element. The `FLAGTEXT` of a
   `flag` and the options of a `one` are the caller's text and pass as written, a leading `-`
   included.
8. **Re-check identity** right before the spawn, in the same directory: a branch's full ref
   still resolves (`git rev-parse --verify --quiet REF^{commit}`); a file or directory still has
   the same `dev`/`ino` under a no-follow stat; a commit OID and a `-` record need no check.
   A mismatch is exit 6 `identity_changed`. The check narrows a race; it cannot close it.
9. **`--dry-run`:** print the quoted command on stdout, one line, through `output::shell_quote`;
   evidence on stderr; exit 0. Quoting works on bytes, so an argument that is not UTF-8 prints
   and reads back exactly. With a machine format the envelope goes to stdout and carries
   `data.argv`; an argv that is not UTF-8 is exit 6 `argv_not_utf8` there, since JSON cannot
   hold it.
10. **Run.** Spawn the argv with inherited stdout and stderr; stdin is `/dev/null` when step 2
    consumed it, inherited otherwise. A spawn error is exit 6 `command_not_found` or
    `spawn_failed`.
    - The child stays in jevify's process group and keeps the terminal. An editor or a pager
      works, the terminal's Ctrl-C and job control reach it directly, and jevify needs no
      terminal handling.
    - Signal handlers are registered immediately before the spawn, never earlier, so Ctrl-C keeps
      its default action during enumeration and requests (jevify dies, nothing ran). From the
      spawn on, jevify waits for the child whatever arrives. TERM and HUP are forwarded to the
      child always. INT is forwarded only when neither stdin nor stderr is a terminal: a
      terminal already delivers it to the whole group, and a second INT makes `cargo`, `pytest`
      and `docker` skip their cleanup.
    - std resets SIGPIPE to its default in the child, so `jevify fill -- git log … | head` ends
      as it does without jevify, with `ran, signal 13`.
    - Status lines are written with `let _ = writeln!(stderr, …)`, so a closed pipe never
      panics. Exit 0 or 7.
11. **Every path ends the same way.** `fill` returns its last line in `Outcome.tail`; `run_cli`
    prints it after any `-v` line. `report_error` and the clap error path print
    `jevify fill: not run: <error kind>` as the last stderr line when the verb is `fill`.
    An agent, or `jevify why` downstream, reads one last line and knows whether anything ran.

`--json` and `--report`. `run_cli` computes `meta` after `dispatch` returns
(`src/lib.rs:151-153`) and prints the envelope on stdout for every machine format
(`:172-188`, errors at `:207-227`). In a run stdout belongs to the child, so a machine format on
`fill` requires `--dry-run`. There is no `--report FILE`; a run reports on stderr.

Status lines, all on stderr, all prefixed `jevify fill:`:

```
jevify fill: branch tp/auth-refactor 0.91 (next 0.04, none 0.02) "move session check into middleware", 3 days; candidates 41
jevify fill: flag --draft: unsure 0.48, left out
jevify fill: ran, exit 0            | ran, exit 101 | ran, signal 15 | not run: arg 3 branch: ambiguous; …
```

Output verbs and the status lines. `jevify fill:` is a reserved prefix. `input::read_stdin` and
`records.rs` drop **every** input line that starts with `jevify fill: `, wherever it stands: the
evidence line is printed before the child runs, so a rule about trailing lines would leave it as
a record on every successful run. The count goes to `data.fill_lines_dropped`, so a log that
holds such a line of its own is not silently shorter; the saved input (2.4) keeps the original
bytes. When the last dropped line is a `not run:` line, the verb exits 3 `upstream_not_run`
whatever else the input holds (clap usage text, a hint). A child that ends its output without a
newline glues the final status to its last line; that line then stays a record, which costs
nothing.

Stable error kinds added by this plan: `stdin_is_tty`, `enumerator_failed`,
`collector_not_allowed`, `too_many_candidates`, `prefix_not_a_directory`, `identity_changed`,
`command_not_found`, `spawn_failed`, `argv_not_utf8`, `ambiguous_runner`, `complete_unsupported`,
`too_many_records` (all exit 6) and `upstream_not_run` (exit 3). An abstention of `fill` is an
`Outcome` with exit 3 and `data.reason`, not an error.

### 2.2 Kinds

Collectors are **inert** or **executing**. An inert collector reads files or queries a tool
without running project code: `git`, `gh` (a network call), `inventory::load`, `package.json`,
`just --summary`. An executing collector runs the project's own code: `cargo test -- --list`
builds every test binary, `pytest --collect-only` imports every test module, `go test -list`
compiles and may run `TestMain`, `TOOL __complete` runs the caller's tool. Executing collectors
need `--allow-collectors`, under `--dry-run` too, because an agent reaches for `--dry-run` to
avoid acting. `capabilities` lists each kind with its collector argv, class, cap and deadline.

| Kind | Collector (argv) | Class | Handle | Tier 1 evidence | Tier 2 | Cap for `pick --from` | Deadline |
|:---|:---|:---|:---|:---|:---|:---|:---|
| `-` | stdin / `--candidates FILE` via `records.rs` | — | `--key` / `--field N` / whole record | `--evidence` fields or whole record | — | 20,000 (`pick::MAX_LINES`) | — |
| `branch` | `git for-each-ref --sort=-committerdate --format=… refs/heads refs/remotes` | inert | local short name; `origin/x` for a remote-only ref | name, tip subject, age computed by code, "(remote only)" | + last 5 subjects, changed top-level paths | 2,000 | 5 s |
| `commit` | `git log -n N --format=… HEAD`, NUL-delimited | inert | full OID | subject | + body, changed paths (`git show --no-ext-diff --stat`) | 2,000 | 5 s |
| `script` | the nearest `package.json` read as a file; `just --summary` | inert | name | name + command text (`package.json`), name (`just`) | — | 500 | 5 s |
| `tool` | `inventory::load` (exists; blocking, stays on `spawn_blocking`) | inert | name | name + whatis line | — | the inventory | — |
| `test` | `cargo test -- --list`, `pytest --collect-only -q`, `go test -list .`, chosen by the marker file in the directory | executing | test id | id | — | 5,000 | 180 s, with `jevify fill: listing tests (cargo test -- --list)` after 2 s |
| `pr`, `issue` | `gh pr list` / `gh issue list` `--state all --limit 200 --json number,title,state,headRefName` | inert, network | number | title, state, branch | + body (`gh … view N --json body`) | 200 | 20 s |
| `ci-run` | `gh run list --limit 200 --json databaseId,displayTitle,status,conclusion,headBranch` | inert, network | run id | title, status, conclusion, branch | — | 200 | 20 s |
| `file`, `dir` | `git ls-files -co --exclude-standard -z`; outside a git work tree a no-follow walk | inert | path relative to the prefix | path | + first lines (`sort::excerpt`, `pub(crate)`, needs an absolute normalized path, returns `name: content`) | 20,000 | 5 s |
| `complete` | `TOOL __complete <resolved argv to the left> <partial argument>` (the Cobra protocol) | executing | completion | completion + description | — | 255 | 5 s |

Rules for every kind:

- In `fill` the limit is `W` for every kind (2.1 step 3); the cap column applies to
  `pick --from`, which prints and may use the tournament. `tool` holds thousands of entries, so
  it serves `route` and `pick --from tool`; in `fill` it meets `too_many_candidates` until
  hunch-zxz.
- `branch` lists one candidate per branch. A remote ref whose local branch exists is folded into
  it, and `origin/HEAD` and other symbolic refs are skipped. `tp/auth` and `origin/tp/auth` have
  the same tip subject and age, and two such twins split the Choice mass and abstain on every
  pushed branch (`src/cmd/pick.rs:44-46` states the same for repeated lines). A detached HEAD
  and an unborn repository are fixtures in the tests.
- `commit` lists `HEAD`. Older commits than the listed ones are reported as omitted, and a
  `no_match` line adds "older commits were not searched; pipe `git log --grep … ` into
  `'@{-:…}'`".
- `test`: two marker files in one directory (`Cargo.toml` and `pyproject.toml`) is exit 6
  `ambiguous_runner`, naming the piped form. jevify never guesses a runner.
- `gh` runs with prompts disabled. Not logged in, no repository and a rate limit are
  `enumerator_failed` with `gh`'s own text, never an empty listing.
- `complete` needs `--allow-collectors`, so jevify keeps no list of tools. It passes the partial
  argument to the left of the marker (`--context=`), reads tab-separated descriptions and drops
  the final `:N` directive line. Output without that directive line is exit 6
  `complete_unsupported`, and the message names `@{one:…}`. One `complete` marker per command;
  it resolves after every other marker, as one further round, the documented exception to the
  two-round rule that the `run` argument pass was.
- A literal prefix in the same argument narrows path kinds. The prefix is the literal text
  before the marker when it ends with `/`: the whole text if it names a directory, else the part
  after the first `=` (`'--config=conf/@{file:…}'`). Neither names a directory: exit 6
  `prefix_not_a_directory`. jevify reads no flag; it only tests which of the two strings is a
  directory.
- `make -qp` evaluates `$(shell …)` and can remake included files; `target` is not built.
  `stash`, `process`, `container`, `pod`, `host` have no collector yet. `'@{-:…}'` over the
  owning tool's listing covers all of them today (see Later).
- No collection is an atomic snapshot across tools. The envelope records scope, candidate
  count, omitted count and the listing digest.

Hidden files. `file` and `dir` list hidden paths, tracked and untracked, as the vision says;
`.git/` is never listed. What is restricted is content: tier-two excerpts are never read for a
path with a dot component, nor for a name matching `.env*`, `*.pem`, `*.key`, `id_*`,
`*credentials*`, `*secret*`, `.netrc`, `.npmrc`, `.envrc`. Those candidates are ranked by their
path alone, and the status line says `excerpts withheld: N`. Symlinks are not followed. Outside
a git work tree there is no ignore list; the envelope says `ignore: none`. `pick --files` keeps
its rule (no dot path at all, `src/cmd/pick.rs:145`).

### 2.3 Records (`records.rs`)

`records.rs` reads bytes through `input::read_stdin_bytes()`. `input::read_stdin` decodes
lossily, strips ANSI, cuts at `\r` and trims each line (`src/input.rs:43-49,69`), and cannot
express NUL separation; it stays as it is for `why` and `is`.

Two independent choices, never a detection order.

- **Splitting**, exactly one of: lines (default), `-0`, `--para`, JSON (chosen by `--key`). Two
  of them together is exit 2. JSON: when the first non-blank byte is `[` the input is one array;
  otherwise each line is one value; a value that fails to parse is exit 6 with its ordinal.
- **Reading a record**, with any splitting: whole record (default); `--field N`, the N-th
  whitespace-separated field, 1-based, as awk, as the handle and the whole record as evidence (a
  record without that field is exit 6 with its ordinal); `--key NAME`, a top-level JSON key
  whose value is a string or an integer, as the handle; `--evidence NAME` (repeatable, needs
  `--key`, else exit 2) for the fields the model reads; `--files`, the record is a path and the
  evidence is its first lines, so `fd -0 | jevify filter -0 --files '…'` works.

Blank records are dropped. Records come out as the bytes they came in, in input order: CRLF,
trailing spaces, colour codes and bytes that are not UTF-8 survive. A selected element of a JSON
array comes out as one JSON line, its original text. Evidence alone is decoded, ANSI-stripped,
clipped and redacted. Byte-identical records are judged once: `filter` and `label` apply the
answer to every occurrence, `pick` returns the first and reports the count. Records that differ
only in their handle are never merged (2.1 step 3). Input above `input::MAX_BYTES` (64 MiB) is
exit 6 `input_too_large`, as today; a 2 GB log never enters memory.

### 2.4 The saved input

`why` and `filter` write the full input, as the bytes they read, to
`<save dir>/outputs/<blake3-16>.log` before the first request, and print `full output: PATH` as
their last stderr line. `pick` returns one record of a list the caller can list again, and
`label` and `is` return every record or none; they save nothing.

- The name is the content hash. The same input maps to the same file, a printed path never
  holds another run's input, and two processes never contend for a name. There is no slot, no
  index file and no lock.
- The write goes through a temporary name and a rename, as `src/jev/cache.rs:42-53` does. When
  the final name exists with the same length the write is skipped. The directory is created
  0700, the file 0600.
- jevify never deletes and never prunes these files. `capabilities` and `PRIVACY.md` name the
  directory; clearing it is the owner's act (section 9).
- `<save dir>` is `JEVIFY_CACHE_DIR` (`src/config.rs:99`; there is no `JEVIFY_CACHE`) or the
  platform cache directory. It does not depend on the answer cache: `--no-cache` and
  `JEVIFY_NO_CACHE` set `cache_dir` to `None` (`src/config.rs:97-98`) and the test harness sets
  it for every test (`tests/common/mod.rs:187,196`). `--no-save` and `JEVIFY_NO_SAVE` skip the
  save; the harness sets `JEVIFY_NO_SAVE=1` by default and the saved-input tests point
  `JEVIFY_CACHE_DIR` at a temporary directory.
- The file holds the raw input, secrets included. It is the one place where jevify keeps text
  that `input::redact` masks on the wire. `PRIVACY.md`, `capabilities` and the skill say so.
- A failed or skipped save prints `full output: not saved (<reason>)` and sets
  `data.complete=false`. A reduction never claims to be complete without its saved input.

## 3. Phases

One shared checkout, `main` only, no branch, no worktree. Every bead reserves its exact write
set through Agent Mail before its first edit. Each phase has the same shape:

1. a serial **skeleton** bead that owns the shared files (`Cargo.toml`, `Cargo.lock`,
   `src/cli.rs`, `src/lib.rs`, `src/cmd/mod.rs`, `src/exit.rs`, `src/input.rs`, `src/output.rs`)
   and lands every clap variant, dispatch arm, final function signature, error kind and module
   stub of the phase, so that the tree passes the quality gate after it;
2. **parallel** beads that each own files nobody else writes in that phase;
3. a serial **integration** bead where one file needs the parallel work;
4. a **contract** bead that owns `src/cmd/agent.rs`, the documents, the skill and the plugin
   manifests, then the version bump and the release.

Two beads that name the same file are serial, in the order given. `.beads/issues.jsonl` has one
writer at a time. Every bead ends with the quality gate of `AGENTS.md`. Every phase ends with
`capabilities`, `docs/ROBOT_MODE.md`, README, guide, a `PRIVACY.md` row, `CHANGELOG.md`, and the
skill triggers of the verbs and kinds that phase delivers and no others. A release goes out only
through `bash scripts/release.sh <full SHA of a successful main CI commit>`. Phase 1 holds every
breaking change and is 0.5.0; later phases add and are minor releases.

### Phase 0 — base URL (hunch-ed1)

Goal: the key never leaves for a host it does not belong to.

`JEVIFY_BASE_URL` must be `https`. The TypeSafe backend accepts only the host
`api.typesafe.ai`, the classifier backend only `classifier.dev`; `127.0.0.1` and `localhost`
accept any scheme, for wiremock. A URL with userinfo is refused. Redirects are not followed
(reqwest follows ten by default).

| Bead | Writes | After |
|:---|:---|:---|
| hunch-ed1 | `src/config.rs`, `src/jev/client.rs`, `tests/client.rs`, `docs/guide/configuration.md`, `PRIVACY.md`, `CHANGELOG.md` | — |

Done when `cargo test --locked --test client -- --test-threads=1` passes with tests for: plain
http refused, unknown host refused, a TypeSafe key with `classifier.dev` refused, userinfo
refused, a 302 not followed, localhost accepted.

### Phase 1 — `fill` with `-` and `branch`, and the grammar cutover (0.5.0)

Goal: an agent replaces "list, read, choose, act" with one call, for any list it can pipe and
for branches; the release holds exactly one verb that starts a command.

In: `marker.rs` without `one` and `flag`; `records.rs` with lines, `--field`, `--key`,
`--evidence`; `source.rs` with `branch` and both tiers; `fill` with `--dry-run`, run, `-C`,
several markers, status lines, signals, the branch identity check; the status-line drop in the
output verbs; `why -- CMD` removed; `run` becomes print-only `route`; `init` points at `route`;
the skill, the plugin manifests and every document move to the new grammar.
Out, on purpose: `--candidates` and `--context` (a pipe covers Phase 1), `one`, `flag`, every
other kind, the saved input, `--para`, `-0`.

| Bead | Writes | After |
|:---|:---|:---|
| 1.0 skeleton — new: fill skeleton and grammar cutover | the shared files of section 3; stubs `src/marker.rs`, `src/records.rs`, `src/source.rs`, `src/cmd/fill.rs`; `src/cmd/why.rs` + `tests/why.rs` (delete `capture`, the `cmd` argument); the `Outcome` change in `src/cmd/{pick,why,is,add,sort,run,agent}.rs`; `run`'s new signature in `src/cmd/run.rs`; `tests/cli_basics.rs` | Phase 0 |
| 1.1 — new: marker lexer | `src/marker.rs` | 1.0 |
| 1.2 — new: byte records | `src/records.rs` | 1.0 |
| 1.3 — hunch-q8p (kinds `-`, `branch`) | `src/source.rs`, `src/tournament.rs`, `tests/tournament.rs` | 1.0 |
| 1.4 — new: route is print-only | `src/cmd/run.rs`, `src/manpage.rs` (`parse_flags` leaves with its only caller), `tests/run_route.rs`, `tests/run_args.rs` (rewritten in place: `route` refuses `--yes` and `--exec`, starts nothing) | 1.0 |
| 1.5 — new: FakeJev probability vectors | `tests/common/mod.rs` | 1.0 |
| 1.6 integration — hunch-bkb (fill core) | `src/cmd/fill.rs`, `tests/fill.rs`, `tests/fill_pipe.rs` | 1.1, 1.2, 1.3, 1.5 |
| 1.7 contract — hunch-sb9 (skill) and new: 0.5.0 documents | `src/cmd/agent.rs`, `tests/agent.rs`, `docs/ROBOT_MODE.md`, `README.md`, `docs/guide/*.md`, `PRIVACY.md`, `CHANGELOG.md`, `AGENTS.md`, `plugins/jevify/skills/jevify/SKILL.md`, `plugins/jevify/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`; last, alone: the version in `Cargo.toml`, `Cargo.lock`, `tests/release.rs` | 1.0; the version bump after 1.4 and 1.6 |

1.1 to 1.5 run in parallel; 1.7 runs beside them and beside 1.6. `src/args.rs` loses its caller
in 1.4. That bead also removes the line `mod args;` from `src/lib.rs`, the one edit to a shared
file outside a skeleton; no other bead of the phase writes `src/lib.rs` after 1.0. The file
`src/args.rs` stays on disk, outside the module tree, until the owner allows its removal
(section 9).

Done when, in this repository, on the default backend:
```
jevify fill --dry-run -- git switch '@{branch:the release automation work}'
git log --format='%H %s' -90 | jevify fill --field 1 -- git show '@{-:made folder moves atomic}'
gh pr list --json number,title | jevify fill --dry-run --key number -- gh pr view '@{-:the Windows path fix}'
jevify fill -- git switch '@{branch:a branch that does not exist}'; echo $?      # 3, nothing ran
jevify fill --dry-run -- git switch '@{branch:x}' 2>&1 | jevify why; echo $?      # 3 upstream_not_run or a record of git, never a fill line
jevify --json fill -- true '@{-:x}' </dev/null; echo $?                          # 2, the message shows --dry-run
jevify route 'inspect Mach-O metadata'                                           # prints a tool, starts nothing
jevify run 'x'; echo $?                                                          # 2
```
and, as contract tests: a branch that exists locally and on `origin` resolves; 100 lines on the
classifier backend are `too_many_candidates` with no request.

### Phase 2 — arguments from a context, `--candidates`, `pick --from`

Goal: an option that depends on text the agent has not read; the handle without the run.

`one`, `flag`, `--context FILE`, `--candidates FILE`, one request for all context markers, the
`flag` rule of 2.1. `pick --from KIND 'description'` over `source.rs` prints the handle and
may use the tournament up to the kind's cap.

| Bead | Writes | After |
|:---|:---|:---|
| 2.0 skeleton | shared files (`--context`, `--candidates`, `Pick --from`) | Phase 1 |
| 2.1 — new: `one` and `flag` lexing | `src/marker.rs` | 2.0 |
| 2.2 — new: `pick --from` | `src/cmd/pick.rs`, `tests/pick.rs` | 2.0 |
| 2.3 integration — new: context markers | `src/cmd/fill.rs`, `tests/fill.rs` | 2.1 |
| 2.4 contract | the contract files of 1.7 | 2.0 |

Done when the `gh issue create` example of the vision, under `--dry-run` with the cache off,
sends exactly one POST for its three markers (`meta.requests == 1`), a helper child receives
the exact argv in a run, an unsure `flag` is left out and the command still runs, and
`jevify pick --from branch 'the release automation work'` prints one branch name.

### Phase 3 — `filter`, record modes, the saved input (hunch-bx6)

Goal: thirty records where there were ten thousand, with a way back.

`filter 'statement'` with the `is` band per record: yes kept, unsure kept and counted, `--strict`
drops the unsure; exit 0 kept some, 1 kept none, 3 every record unsure (also under `--strict`).
`--para`, `-0`, `--files`, JSON in every output verb. The saved input for `why` and `filter`.
The scorer sends 20 records per request (the classifier.dev limit; `add.rs:10` uses the same
number and is not touched). Cost control: `filter` prints `N records, R requests` on stderr
before its first request and refuses more than 2,000 records with exit 6 `too_many_records`
and no request; `--max-records N` raises the limit, and the message gives the exact command.
There is no request-budget variable: `why` alone sends up to 21 requests on a long log
(`MAX_KEEP = 4000`, `src/cmd/why.rs:14`), so a default of 8 would break it, and exit 4 invites a
retry. A daily-quota 429 is never retried.

| Bead | Writes | After |
|:---|:---|:---|
| 3.0 skeleton | shared files (`Filter`, record options on every output verb, `--no-save`), `src/config.rs` (save dir) | Phase 2 |
| 3.1 — new: record modes | `src/records.rs` | 3.0 |
| 3.2 — new: saved input | `src/save.rs` (new: content-addressed write, used by two verbs), `tests/common/mod.rs` (`JEVIFY_NO_SAVE`) | 3.0 |
| 3.3 — hunch-bx6 | `src/cmd/filter.rs`, `tests/filter.rs` | 3.1, 3.2 |
| 3.4 — new: records and save in `why`, records in `pick` | `src/cmd/why.rs`, `tests/why.rs`, then `src/cmd/pick.rs`, `tests/pick.rs` | 3.1, 3.2 |
| 3.5 contract | the contract files of 1.7 | 3.0 |

Done when `seq 1 3000 | jevify filter 'x'` is exit 6 with the `--max-records 3000` line and no
request; coloured `rg` output comes out byte-identical; two runs over the same input print the
same path; an unwritable save directory gives `not saved` and `data.complete=false`.

### Phase 4 — `commit`, `script`, `tool`, `test`

`commit` with both tiers and the newest-`W` rule; `script`; `tool` for `pick --from tool`, which
calls the `route` code; `test` for cargo, pytest and go behind `--allow-collectors`.

| Bead | Writes | After |
|:---|:---|:---|
| 4.0 skeleton | shared files (`--allow-collectors`) | Phase 3 |
| 4.1 — hunch-q8p, continued: `commit`, `script`, `tool` | `src/source.rs` | 4.0 |
| 4.2 — new: `test` collectors | `src/source.rs` | 4.1 (same file, serial) |
| 4.3 — tests and wiring | `src/cmd/fill.rs`, `tests/fill.rs`, `src/cmd/pick.rs`, `tests/pick.rs` | 4.2 |
| 4.4 contract | the contract files of 1.7 | 4.0 |

Done when `jevify fill --dry-run -- git show '@{commit:made folder moves atomic}'` prints a full
OID, `jevify fill --dry-run -- cargo test '@{test:the retry backoff cap}'` is exit 6
`collector_not_allowed` and resolves with `--allow-collectors`, and collector tests with fake
`git`, `cargo` and `pytest` first on the PATH assert argv, directory, deadline, the killed child
and the bounded stderr tail.

### Phase 5 — `label` and several statements (hunch-qxn)

`label a,b,c`: one Choice per record, the options plus an internal `NONE` id, 20 records per
request through the scorer of `filter.rs`. Labels are split on `,` and trimmed; an empty or
repeated label is exit 2; a label that holds a comma is given with `--label` repeated. Output is
`LABEL<TAB>RECORD` with the record's own terminator; an unsure record has the label `?`; JSON
records come out as `{"label":…,"p":…,"record":<the original value>}` lines.
`label --ordered low,mid,high` is the same Choice with an instruction that names the labels as
a scale in the given order, and the output adds the 1-based rank. `src/jev/mod.rs:18-28` has
Noul and Choice only; no Score wire type is added.
`is 'a' 'b' 'c'`: one Noul per statement in one request, one verdict line per statement; exit 1
when any is no, else 3 when any is unsure, else 0. One statement behaves as today: no stdout.

| Bead | Writes | After |
|:---|:---|:---|
| 5.0 skeleton | shared files (`Label`, `Is` with several statements) | Phase 3 |
| 5.1 — hunch-qxn | `src/cmd/label.rs`, `tests/label.rs` | 5.0 |
| 5.2 — new: `is` with several statements | `src/cmd/is.rs`, `tests/is.rs` | 5.0 |
| 5.3 contract | the contract files of 1.7 | 5.0 |

Phases 4 and 5 share only skeleton and contract files; their skeletons run one after the other,
the rest in parallel.

### Phase 6 — `file`, `dir`, `pr`, `issue`, `ci-run`

Prefix scoping and prefix-relative substitution, hidden paths with withheld excerpts, the
`dev`/`ino` identity check, the three `gh` kinds.

| Bead | Writes | After |
|:---|:---|:---|
| 6.1 — new: path kinds | `src/source.rs`, then `src/cmd/fill.rs`, `tests/fill.rs` | Phase 4 |
| 6.2 — new: `gh` kinds | `src/source.rs`, `tests/fill.rs` | 6.1 (same files, serial) |
| 6.3 contract | the contract files of 1.7 | 6.1 |

Done when `jevify fill --dry-run -- wc -l 'src/cmd/@{file:stages hunks}'` prints
`'wc' '-l' 'src/cmd/add.rs'`, a repository with a tracked `.npmrc` reports
`excerpts withheld: 1` and the request body captured by wiremock holds none of its content, and
a fake `gh` that prints "not logged in" gives `enumerator_failed` with that text.

### Phase 7 — `complete`

The Cobra protocol behind `--allow-collectors`, after every other marker.
Writes: `src/source.rs`, `src/cmd/fill.rs`, `tests/fill.rs`, the contract files. Serial.
Done when a fake Cobra tool resolves `'--context=@{complete:the staging cluster}'` and a fake
tool without the directive line is `complete_unsupported`.

### Later

Each item names what would make it worth building.

- `fill` over several windows (hunch-zxz): agents meet `too_many_candidates` on lists they
  cannot narrow. It needs a tournament that keeps every rival of the winner, not a window count
  in the status line.
- `--report FILE` for a run: a harness needs the resolution data of a real run in machine form.
  `run_cli` would write it after `meta` is complete; `fill` would only return the path.
- `JEVIFY_MAX_REQUESTS`, optional, no default: a user reports an unexpected bill or an exhausted
  daily quota.
- `--show-sent`: a user asks what a kind sends and `capabilities` does not answer it.
- A Score wire type: ordered labels through Choice prove unstable between neighbours.
- `--strict` on `fill` (an unsure `flag` abstains): a caller shows a flag whose omission is the
  harmful direction.
- A setting for the ratio of 2.1: abstention reports show one constant does not fit both
  backends.
- Forwarding signals to a child's process group: a harness reports orphaned grandchildren after
  TERM.
- `target` (`make`), `stash`, `process`, `container`, `pod`, `host`: a user asks for one and the
  piped `'@{-:…}'` form is shown to be too clumsy.
- Pruning of saved inputs: only with the owner's permission; the project forbids deletion.
- A post-failure hook that appends `why` to a failed long command. `@{+kind:…}` for several
  handles.

## 4. clap and parsing

- `fill`: `#[arg(last = true)] cmd: Vec<OsString>`; a command without `--` is exit 2 with the
  corrected example. `trailing_var_arg` stays off so a misplaced option is an error. A second
  `--` inside the command passes through; a test holds it.
- Output verbs do not take `allow_hyphen_values`: it would swallow a misspelled jevify flag as a
  statement. Text that starts with `-` goes after `--`, as the vision says, and the usage error
  shows that form.
- `pick -n` and `why -n` keep "top N". `fill` has `--dry-run` only, no `-n`. `-C` is a directory
  on `fill` and context lines on `why`; both help texts say so.
- All positionals that reach a child are `OsString`.
- Near-miss tolerance: `jevify fill git switch …` (no `--`) gives exit 2 with one corrected
  line; jevify never runs a guess.
- `is` takes one or more statements (`Cmd::Is { condition: String }` today, `src/cli.rs:110`).
  One statement keeps its exact behaviour, so the change adds and breaks nothing.

## 5. Tests

- `marker.rs` table: every literal from the audit (`@{u}`, `@{-1}`, `HEAD@{2}`, `stash@{0}`,
  `@{1 day ago}`, `@types/node`, `user@host:path`, PowerShell `@{k='v'}`, Python
  `{user}@{host:>8}` which must be written `@@{`), prefix, suffix, flag value, short flag value,
  two markers in one argument, `\}`, quotes stripped once, unterminated, unknown kind with
  suggestion and the list of kinds, empty body, a marker in `argv[0]`, non-UTF-8 literal passing
  through, `one` and `flag` lexing, a `one` with one option or a repeated option, substituted
  text that holds `@{`.
- `FakeJev` gives the winner 0.9 and spreads 0.1 over the rest
  (`tests/common/mod.rs:26-31`), so no tie, no near miss and no `ambiguous` path can be
  written today. Bead 1.5 adds a responder that returns a probability vector per question;
  `FakeClassifier` is built from it, so both backends get it.
- `tests/fill.rs`: dry-run output equals the argv a run receives (a helper child prints its argv
  as NUL-separated bytes), with a non-UTF-8 literal too; abstain prints nothing and spawns
  nothing (a sentinel file proves no child ran); 0, 1 and 2 candidates; a tie; `NONE` close to
  the best; `any` below the threshold; same evidence with different handles is `ambiguous`; a
  handle with a newline or a leading `-` is omitted and counted, and passes after a literal
  prefix; `W` and `W + 1` candidates on each backend; round 2 runs only when the ratio fails and
  sends at most 24 candidates; a branch with a remote twin resolves; several markers, one
  snapshot; stdin ownership (the child sees EOF with `@{-:…}`, inherits otherwise); `-C DIR`
  moves collectors and the child; `argv[0]` missing sends no request; a machine format without
  `--dry-run` is exit 2; every exit 2–6 ends with a `not run:` line, clap errors included;
  exit 0 and 7; `ran, signal N`; TERM reaches the child; a child that reads the terminal is not
  stopped (run under a pty when the test host has one, else reported as not run); a closed
  stderr pipe does not panic; a branch deleted between resolve and spawn is exit 6.
  Phase 2 adds: `one` + `flag` in one request; `flag` no and unsure remove the element; a `flag`
  answer on the classifier backend; an oversized context sends nothing; `--candidates` leaves
  stdin to the child.
- `tests/fill_pipe.rs`: the evidence line of a successful `fill` is not a record of `why`,
  `pick` or `is`; `upstream_not_run` behind clap text; a child line that starts with the
  reserved prefix is dropped and counted.
- Output verbs (Phase 3): record modes and their exit-2 combinations; CRLF, colour, NUL and
  non-UTF-8 records come out byte-identical; a JSON array element comes out as one JSON line;
  the saved path under `JEVIFY_NO_CACHE=1`; two saves of the same input; 20 concurrent saves;
  the failed-save path; `--no-save`; 64 MiB plus one byte.
- Collector tests put fake `git`, `gh`, `cargo` executables first on the PATH and assert argv,
  directory, deadline, the kill, the stderr tail and the error kind.
- Live tests (ignored by default) for one `branch` and one `one` resolution per backend. Without
  a key they are reported as not run, never as passed.

## 6. The skill file

The file is `plugins/jevify/skills/jevify/SKILL.md`. It teaches `jevify run` and
`why -- CMD` today; bead 1.7 rewrites it, and each phase adds the triggers of what it delivers.
The skill never names a verb or a kind the installed release lacks. `plugin.json` and
`marketplace.json` carry the same version as the crate.

Triggers, one line each: about to list branches, commits, tests, PRs or runs only to choose one →
`fill`; can describe it, cannot spell it, and a tool can list it → pipe the list into
`fill … '@{-:…}'`; want the value and not the run → `pick --from KIND`; an option depends on a
ticket, diff or log you have not read → `@{one:…}` / `@{flag:…}` with the text on stdin; a
failed build or test with more than about 50 lines → `2>&1 | jevify why`; many records and one
yes/no question → `filter`; every record needs a bucket → `label`; the next step depends on a
fact about some text → `is … &&`. Skip jevify for a name already seen, a literal search, counts
and dates, and untrusted text.

Habits: the whole marker argument in single quotes with no quotes inside; an apostrophe is
`'\''`; run `fill` directly and use `--dry-run` to look, never `eval` its output;
`set -o pipefail` and `2>&1` before `|`; a marker does not survive a second shell (`ssh`,
`make`, `watch`, `xargs`), so `fill` goes on the side where the command runs; `route` never
runs anything, `fill` does; a marker that reads stdin leaves the command an empty stdin.

Recovery, the same table in the skill and in `capabilities`:

| Seen | Do |
|:---|:---|
| exit 2, marker error | copy the corrected argument from the message |
| `too_many_candidates` | add a literal path prefix, or pipe a narrower list into `'@{-:…}'` |
| exit 3 `ambiguous` | read the two closest handles in the status line; pick one and write it, or sharpen the description |
| exit 3 `no_match` | the thing is not in the listed scope; read `candidates N of M` |
| `N candidates share the same evidence` | add `--evidence` fields |
| `collector_not_allowed` | add `--allow-collectors`, or pipe the lister yourself |
| a machine format on a run | add `--dry-run`, or drop `--json` and read stderr |
| the command needs stdin | `--candidates FILE` / `--context FILE` |
| `enumerator_failed` | run the named collector yourself and read its error |

Permissions: allow the output verbs and `jevify fill --dry-run` freely (inert collectors still
run, and `gh` is a network call). `jevify fill` runs what the caller wrote, so a rule such as
`Bash(jevify fill:*)` allows every command. Allow it per command prefix
(`jevify fill -- git switch:*`), exactly as the command itself is allowed.

## 7. Privacy and cost

The description, the context of `one` and `flag`, and the declared evidence fields of
candidates leave the machine, masked on a best-effort basis by `input::redact`. Paths of hidden
files leave as names; their content does not (2.2). `meta` reports requests (attempted POSTs,
retries included, `src/config.rs:197`), cache hits and classifications; a warm cache can make
`meta.requests` zero, which is why the Phase 2 test turns the cache off.

Cost is bounded by structure, not by a budget variable: a `fill` sends at most two requests per
listing marker and one for all context markers; `pick --from` is bounded by the kind's cap;
`filter` and `label` print `N records, R requests` before the first request and stop at 2,000
records unless `--max-records` raises it. A daily-quota 429 is never retried; other 429s keep
the retry policy of `src/jev/client.rs:418-422`.

Two stores exist and the documents keep them apart: the answer cache (redacted requests and
responses, seven days, `--no-cache`) and the saved inputs (raw, never pruned by jevify,
`--no-save`).

## 8. Risks

1. **A wrong pick runs.** Guards: the `NONE` option, the `any` Noul, the ratio against both the
   runner-up and `NONE`, one window only, the tier-two round, one candidate per branch, the
   identity re-check, all-or-nothing, the evidence line the agent reads. No effect classes; the
   harness permission system owns that decision.
2. **Agents misquote the marker.** The lexer turns every damaged marker it can see into exit 2
   with the corrected spelling, and zero markers is an error. A shell can still leave a valid
   marker with a damaged description (`$x` expanded inside double quotes); the lexer cannot see
   that, and the skill's single-quote habit is the guard.
3. **Collectors are slow or run code.** The inert/executing split, `--allow-collectors`, a
   deadline per kind with kill and reap, the collector argv in `capabilities`. `--dry-run` runs
   inert collectors and says so.
4. **`fill` refuses long lists.** One window is 99 candidates on the keyless backend. The
   message teaches the prefix and the pipe, ordered kinds keep the newest `W`, and
   `pick --from` searches up to the cap without running anything.
5. **The keyless backend** takes 99 options, 20 questions per request and fewer classifications
   per day. Per-record verbs state their request count before they start.
6. **Saved inputs hold secrets and grow.** 0600 in a 0700 directory, named in `PRIVACY.md`,
   `--no-save`; jevify never deletes them.
7. **A child prints the reserved prefix.** The line is dropped and counted in
   `data.fill_lines_dropped`; the saved input keeps it.
8. **A grandchild outlives a TERM.** jevify signals the child only (Later).

## 9. Owner decisions

1. `git mv src/cmd/run.rs src/cmd/route.rs`, `tests/run_route.rs` → `tests/route.rs`, and the
   removal of `src/args.rs` and `tests/run_args.rs`. Without permission the verb is renamed, the
   files keep their names, `src/args.rs` stays on disk outside the module tree, and
   `tests/run_args.rs` holds the "route starts nothing" tests.
2. An unsure `flag` is left out and the command runs (2.1 step 5). The alternative, abstain, stops
   the vision's three-argument example whenever one Noul lands in the band.
3. `fill` resolves within one window: 200 candidates with a key, 99 without; ordered kinds keep
   the newest `W`; `tool` serves `route` and `pick --from` only (2.1 step 3, hunch-zxz).
4. Saved inputs are content-addressed and never pruned by jevify (2.4).
5. A machine format on a `fill` run is exit 2; there is no `--report FILE` (2.1).

## 10. Beads

| Phase | Existing beads | New beads |
|:---|:---|:---|
| 0 | hunch-ed1 (extended: backend bound to host, no redirects) | — |
| 1 | hunch-bkb (fill core, bead 1.6), hunch-q8p (kinds `-`, `branch`, bead 1.3), hunch-sb9 (skill, bead 1.7; its text names `skills/`, the path is `plugins/jevify/skills/jevify/SKILL.md`) | fill skeleton and grammar cutover (1.0); marker lexer (1.1); byte records (1.2); route is print-only (1.4); FakeJev probability vectors (1.5); 0.5.0 documents (1.7) |
| 2 | — | `one` and `flag` lexing; `pick --from`; context markers; contract |
| 3 | hunch-bx6 (filter) | record modes; saved input; records and save in `why` and `pick`; contract |
| 4 | hunch-q8p continued (`commit`, `script`, `tool`) | `test` collectors and `--allow-collectors`; contract |
| 5 | hunch-qxn (label) | `is` with several statements; contract |
| 6 | — | path kinds; `gh` kinds; contract |
| 7 | — | `complete` |
| Later | hunch-zxz (several windows for `fill`) | one bead per Later item when its trigger occurs |

Close as superseded: hunch-k6s, hunch-x36, hunch-4z4 (replaced by this plan's epic, hunch-vq2).
Close as obsolete with the argument pass, in Phase 1 (bead 1.4): hunch-gjf, hunch-eil, hunch-u07,
hunch-zss.

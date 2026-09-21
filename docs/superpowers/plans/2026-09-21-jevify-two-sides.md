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
| `is` | output | stdin, or `--context FILE` | no | 0 all yes, 1 one no, 3 otherwise |
| `add`, `sort` | acting verbs, unchanged | — | git apply / rename only | unchanged |

Design rules that every section below applies:

1. **Selection.** Every record a verb prints is a record of its input, byte for byte. Every
   value `fill` substitutes is a handle that a lister printed or the caller wrote.
2. **Time is the cost.** A judgment is almost free; a round trip is not. A verb asks everything
   it can in one round, in parallel, and uses at most two rounds. Limits exist to bound time and
   memory, never to save requests: one ceiling of 20,000 records holds in every verb
   (`pick::MAX_LINES`), and the input cap stays 64 MiB.
3. **Asymmetric doubt.** A reducing verb (`filter`) keeps what it is unsure about, because a
   dropped record costs the whole input. An acting verb (`fill`, `pick`) abstains, because a
   wrong act costs more than no act. Doubt never chooses for the caller, in either direction.
4. **Twin flags.** A flag letter means what it means on the verb's classical twin: `filter` and
   `why` follow `grep`, `pick -n` follows `head`, `fill -C` follows `git -C`.
5. **One role for stdin.** A call never reads stdin for two purposes.

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
  marker in `argv[0]` (the command name is always literal), and stdin with two roles: a `-`
  marker without `--candidates FILE` next to a `one` or `flag` marker without `--context FILE`.
  The message names both options. The number of markers has no limit; context markers beyond 20
  go out as further requests of the same round.
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
| `src/source.rs` | `enumerate(kind, scope) -> Listing { records, total, omitted, scope }`; the recipe engine (2.2): run a lister argv, split and read its output through `records.rs`, fetch tier two through the recipe's `more` argv; the coded kinds; identity; a cap, a deadline and a collector class per kind. Enumerators run an argv, never a shell; a timed-out collector is killed and reaped. | inline + `tests/fill.rs` |
| `src/kinds.jsonl` | The recipes jevify ships, one JSON object per line, compiled in with `include_str!`. Data, not code: a new built-in kind is one line and one test row. | the recipe table test in `source.rs` |
| `src/cmd/fill.rs` | Orchestration: lex → read inputs → enumerate → resolve → substitute → re-check → print or spawn. The only `Command::spawn` for a caller's command. | `tests/fill.rs`, `tests/fill_pipe.rs` |
| `src/cmd/filter.rs`, `src/cmd/label.rs` | Per-record verbs over `records.rs`; the per-record scorer lives in `filter.rs`, and `label.rs` and `is` with several statements use it. `add` keeps its own batches. Both verbs write records to stdout as answers arrive (2.3). | `tests/filter.rs`, `tests/label.rs` |
| `src/save.rs` | The content-addressed saved input of 2.4, used by `why` and `filter`. | inline + `tests/filter.rs` |

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
| `src/tournament.rs` | `window` becomes `pub(crate)` (`:55`); `decide(&Ranking) -> Decision` holds the found rule of 2.1. `rank` changes in one place: the pool of window finalists is cut only when it exceeds one window (`:126-127` sorts the pool by probabilities of different requests and keeps 24). Below that size every finalist meets in the finals, so no cross-request comparison decides anything. `Ranking` gains `windows` and `pool_cut: bool`; `fill` refuses a ranking with `pool_cut`, `pick` reports it |
| `src/cmd/why.rs` | delete `capture` and the `cmd` argument (`:59-`); later the saved input |
| `src/cmd/run.rs` | becomes `route`: delete execution, the confirmation, the argument pass; print tool, summary, synopsis. The file is renamed only with the owner's permission (section 9) |
| `src/cmd/pick.rs` | `--from KIND`; later the record model |
| `src/cmd/is.rs` | several statements; `--context FILE` |
| `src/cmd/agent.rs` | capabilities, robot-docs, `init`; `init agents` prints the `AGENTS.md` block of section 6 |
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
   `--context FILE` or stdin. Step 1 has refused a call where both families need stdin. Files
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
   - More than `W` candidates: the list is searched through windows (step 4). The sound limit
     is `F = W × (W / 3)`: 3,267 candidates on classifier.dev, 13,200 on TypeSafe. Up to `F`
     the three finalists of every window fit one finals request, so the pool is never cut.
     Above `F`, a kind with a recency order (`branch` by committer date, `commit`, `pr`,
     `issue`, `ci-run`) keeps the newest `F` and reports `candidates 3267 of 9120, newest
     first`. Every other kind is exit 6 `too_many_candidates` with no request; the message
     gives the count, `F`, and the two ways to narrow: a literal path prefix, or a narrower list
     piped into `'@{-:…}'`. `pick --from` prints and may cut the pool, up to the ceiling.
4. **Resolve**, at most two rounds. Every request of a round is in flight at the same time,
   across all markers.
   - Round 1. Each listing marker sends one `tournament::window` call per window of `W`
     candidates over tier-one evidence (one Choice with `NONE`, one `any` Noul). All `one` and
     `flag` markers go out over the context: one Choice (the options plus `NONE`) per `one`,
     one Noul per `flag`, 20 questions per request (`MAX_DIMENSIONS`,
     `src/jev/classifier.rs:26`), further requests in the same round beyond 20.
   - Round 2, per listing marker, one request. With several windows it is the finals: the three
     best of each window **by rank**, all of them, with the richest evidence that fits the
     request (tier two for the first 24, `MAX_FINALISTS`, tier one for the rest). Rank inside a
     window is meaningful; a probability is never compared with one from another request, so
     the winner and every rival that beat the rest of its window meet in one Choice. With one
     window, round 2 runs only when the ratio below failed and the kind has tier-two evidence:
     tier two for the top 24 of that window, asked once more.
   - A window whose `any` is below the threshold still sends its finalists. The `any` of the
     finals decides.
   - `rank` with `finalist_text` always runs a finals round (`src/tournament.rs:110-144`);
     `fill` calls `window` directly for the one-window case, so a decisive first round costs
     one request.
5. **Decide** per marker.
   - Listing kinds: found when `any ≥ threshold` and
     `p(best) ≥ 2 × max(p(second), p(NONE))`. A ratio holds for 3 options and for 200; a
     difference does not, because a Choice over 200 options spreads its mass. One candidate has
     `p(second) = 0`. Equal scores abstain. The factor 2 is a constant in `tournament.rs` and is
     reported in the status line and in `data`; it is a guard, not a calibration.
   - `one`: the same ratio over the options and `NONE`; no Noul.
   - `flag`: `is::band_verdict` with the default band (`src/cmd/is.rs:54-62`). Yes keeps the
     argument, no leaves it out, and the status line says which. **Unsure abstains**: nothing
     runs, and the line is `not run: arg 5 flag --draft: unsure 0.48; write --draft or drop the
     marker`. Leaving a flag out is an act (`--dry-run`, `--draft`, `--no-verify`), so doubt
     does not choose it. The caller decides in one turn with the probability in hand.
   - Reasons for exit 3, in `data.reason` and in the status line: `no_match` (`any` below the
     threshold, or `NONE` wins), `ambiguous` (the ratio fails after round 2), `unsure_flag`,
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
jevify fill: commit 3f9c2ab 0.88 (next 0.06, none 0.01) "make folder moves atomic"; candidates 1432, windows 15
jevify fill: flag --draft: no 0.07, left out
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
`recipe_invalid`, `too_many_records` (all exit 6) and `upstream_not_run` (exit 3). An abstention of `fill` is an
`Outcome` with exit 3 and `data.reason`, not an error.

### 2.2 Kinds

Collectors are **inert** or **executing**. An inert collector reads files or queries a tool
without running project code: `git`, `gh` (a network call), `inventory::load`, `package.json`,
`just --summary`. An executing collector runs the project's own code: `cargo test -- --list`
builds every test binary, `pytest --collect-only` imports every test module, `go test -list`
compiles and may run `TestMain`, `TOOL __complete` runs the caller's tool. Executing collectors
need `--allow-collectors`, under `--dry-run` too, because an agent reaches for `--dry-run` to
avoid acting. `capabilities` lists each kind with its collector argv, class, cap and deadline.

Coded kinds, the ones that need logic:

| Kind | Collector (argv) | Class | Handle | Tier 1 evidence | Tier 2 | Deadline |
|:---|:---|:---|:---|:---|:---|:---|
| `-` | stdin / `--candidates FILE` via `records.rs` | — | `--key` / `--field N` / whole record | `--evidence` fields or whole record | — | — |
| `branch` | `git for-each-ref --sort=-committerdate --format=… refs/heads refs/remotes` | inert | local short name; `origin/x` for a remote-only ref | name, tip subject, age computed by code, "(remote only)" | + last 5 subjects, changed top-level paths | 5 s |
| `commit` | `git log -n N --format=… HEAD`, NUL-delimited | inert | full OID | subject | + body, changed paths (`git show --no-ext-diff --stat`) | 5 s |
| `script` | the nearest `package.json` read as a file; `just --summary` | inert | name | name + command text (`package.json`), name (`just`) | — | 5 s |
| `tool` | `inventory::load` (exists; blocking, stays on `spawn_blocking`) | inert | name | name + whatis line | — | — |
| `test` | `cargo test -- --list`, `pytest --collect-only -q`, `go test -list .`, chosen by the marker file in the directory | executing | test id | id | — | 180 s, with `jevify fill: listing tests (cargo test -- --list)` after 2 s |
| `file`, `dir` | `git ls-files -co --exclude-standard -z`; outside a git work tree a no-follow walk | inert | path relative to the prefix | path | + first lines (`sort::excerpt`, `pub(crate)`, needs an absolute normalized path, returns `name: content`) | 5 s |
| `complete` | `TOOL __complete <resolved argv to the left> <partial argument>` (the Cobra protocol) | executing | completion | completion + description | — | 5 s |

Recipe kinds, the `-` form with a name. A recipe is one JSON object on one line:

| Field | Meaning | Default |
|:---|:---|:---|
| `kind` | the name in the marker, `[a-z][a-z-]*` | required |
| `list` | the lister argv | required |
| `split` | `lines`, `nul`, `para` or `json` | `lines`, or `json` when `key` is set |
| `field` / `key` | the handle, as `--field N` / `--key NAME` of 2.3 | the whole record |
| `evidence` | the JSON fields the model reads | the whole record |
| `more` | an argv with one `{}` that prints tier-two evidence for one handle | none |
| `ordered` | the lister prints newest first, so a list above `F` keeps its head | `false` |
| `network` | the lister makes a network call; the deadline is 20 s in place of 5 s | `false` |

```
{"kind":"pr","list":["gh","pr","list","--state","all","--limit","1000","--json","number,title,state,headRefName"],"key":"number","evidence":["title","state","headRefName"],"more":["gh","pr","view","{}","--json","body"],"ordered":true,"network":true}
{"kind":"issue","list":["gh","issue","list","--state","all","--limit","1000","--json","number,title,state"],"key":"number","evidence":["title","state"],"more":["gh","issue","view","{}","--json","body"],"ordered":true,"network":true}
{"kind":"ci-run","list":["gh","run","list","--limit","1000","--json","databaseId,displayTitle,status,conclusion,headBranch"],"key":"databaseId","evidence":["displayTitle","status","conclusion","headBranch"],"ordered":true,"network":true}
{"kind":"stash","list":["git","stash","list","--format=%gd%x09%gs"],"field":1,"ordered":true}
{"kind":"process","list":["ps","-axo","pid=,comm=,args="],"field":1}
{"kind":"container","list":["docker","ps","--format","{{.Names}}\t{{.Image}}\t{{.Status}}"],"field":1}
{"kind":"pod","list":["kubectl","get","pods","--no-headers"],"field":1}
```

- The shipped recipes live in `src/kinds.jsonl` and are inert: each lists through a tool that
  reads state and runs no project code. User recipes come from `kinds.jsonl` in the
  configuration directory (`directories::ProjectDirs`, already a dependency). They run a command
  the user wrote, so they are executing collectors. A user recipe cannot replace a coded kind or
  a shipped recipe; a line that tries, a line that does not parse and an unknown field are
  exit 6 `recipe_invalid` with the line number, when that kind is used and in `capabilities`.
- jevify reads no recipe from the working directory. A cloned repository never adds a command
  that `fill` runs.
- The engine is the `-` path: run `list` under the deadline, hand the bytes to `records.rs` with
  the recipe's split and reading, and get the same `Listing`. `more` runs once per finalist that
  gets tier two, in parallel, with `{}` replaced by the handle as one argv element.
- A kind that needs a tool that is not on the PATH is exit 6 `enumerator_failed` naming the
  tool, never an empty listing.

Rules for every kind:

- In `fill` the limit is `F` for every kind (2.1 step 3); `pick --from` searches up to the
  ceiling of 20,000 and reports a cut pool. `tool` holds a few thousand entries, so it fits
  `fill` on both backends.
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
- Every collector runs with stdin at `/dev/null`, `GH_PROMPT_DISABLED=1`, `GIT_TERMINAL_PROMPT=0`
  and `NO_COLOR=1`, so none of them waits for a person. Not logged in, no repository and a rate
  limit are `enumerator_failed` with the tool's own text, never an empty listing.
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
  `host` is not shipped: its list is the user's SSH configuration, which jevify does not read.
  A user who wants either writes the recipe.
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

The per-record scorer (`filter`, `label`, `is` with several statements):

- **Judge once.** With line splitting, two records that differ only in a leading timestamp
  (ISO 8601, syslog, or a bracketed clock time at the start of the line) share one judgment, as
  byte-identical records do. Nothing else is normalised: a number inside a line can be the
  whole meaning (`exit code 1`). The status line reports `N records, M distinct`.
- **Batches.** 20 records per request, each record one question over a shared state, the form
  `add.rs:10` uses. `filter -e A -e B` asks both statements of a record in the same request,
  so a batch holds 10 records. All batches are queued at once; the client semaphore
  (`src/jev/client.rs:140`) bounds what is in flight.
- **Flow.** A record is written to stdout as soon as every record before it has an answer.
  `filter 'x' | head -5` prints early, and a closed stdout drops the queued requests and ends
  the verb with exit 0. With a machine format nothing flows; the envelope comes at the end.
  The verb writes stdout itself; `Outcome.human` stays empty.
- **The ceiling.** More than 20,000 distinct records is exit 6 `too_many_records` with no
  request; `--max-records N` raises it and the message gives the exact command. Below the
  ceiling the verb never refuses: it prints `jevify filter: 10074 records, 3120 distinct, 156
  requests` on stderr before the first request and does the work.
- **The free backend's pace.** A 429 that is not the daily quota waits and retries inside the
  verb (`src/jev/client.rs:418-422`). A daily-quota 429 ends the verb with exit 4; the records
  already printed are a correct prefix, and the last stderr line says `answered 6200 of 10074`.
- **`filter` flags, as `grep`.** `-v` keeps the records where the statement is false; unsure
  records are kept in both directions unless `--strict`. `-c` prints the count of kept
  records. `-n` prefixes `N:` with the record's 1-based ordinal (`N-` on a context record).
  `-A`, `-B`, `-C` print neighbour records that are never judged, with `--` between groups.
  `-e STATEMENT`, repeatable, keeps a record when any statement holds. `-n` and the context
  separator change the output, so they are the caller's explicit choice; without them records
  come out byte for byte.

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

The order is the language first. Phase 1 lands the grammar and the whole record algebra, which
every later verb reads and which starts no command. Phase 2 lands `fill` on that base. Phases 3
to 5 add kinds and verbs to a language that no longer changes shape.

### Phase 1 — the language: grammar cutover, records, `filter`, `is` (0.5.0)

Goal: one grammar, one record model, and the output side an agent uses most: thirty records
where there were ten thousand, with a way back, and a yes or no in an exit code.

In: the grammar cutover (`why -- CMD` removed; `run` becomes print-only `route`; `init` points
at `route`; no verb of the release starts a caller's command); `records.rs` complete (lines,
`-0`, `--para`, JSON, `--field`, `--key`, `--evidence`, `--files`); the scorer with judge-once,
batches, flow and the ceiling (2.3); `filter` with the `is` band per record and the `grep`
flags; `is` with several statements and `--context FILE`; the saved input for `why` and
`filter` (2.4); records in `why` and `pick`; the README, the skill and every document in the new
grammar (section 6).
Out, on purpose: `fill`, markers, kinds, `label`.

`filter 'statement'`: yes kept, unsure kept and counted, `--strict` drops the unsure; exit 0
kept some, 1 kept none, 3 every record unsure (also under `--strict`).
`is 'a' 'b' 'c'`: one Noul per statement in one request, one verdict line per statement; exit 1
when any is no, else 3 when any is unsure, else 0. One statement behaves as today: no stdout.
`--context FILE` reads the file in place of stdin, with the same budget rule
(`src/cmd/is.rs:21-31`).

| Bead | Writes | After |
|:---|:---|:---|
| 1.0 skeleton — new: language skeleton and grammar cutover | the shared files of section 3 (`Filter`, `Route`, `Why` without `cmd`, `Is` with several statements and `--context`, record options on every output verb, `--no-save`, `--max-records`); `src/config.rs` (save dir); stubs `src/records.rs`, `src/save.rs`, `src/cmd/filter.rs`; `src/cmd/why.rs` + `tests/why.rs` (delete `capture`, the `cmd` argument); the `Outcome` change in `src/cmd/{pick,why,is,add,sort,run,agent}.rs`; `run`'s new signature in `src/cmd/run.rs`; `tests/cli_basics.rs` | Phase 0 |
| 1.1 — new: byte records, every mode | `src/records.rs` | 1.0 |
| 1.2 — new: saved input | `src/save.rs`, `tests/common/mod.rs` (`JEVIFY_NO_SAVE`) | 1.0 |
| 1.3 — new: route is print-only | `src/cmd/run.rs`, `src/manpage.rs` (`parse_flags` leaves with its only caller), `tests/run_route.rs`, `tests/run_args.rs` (rewritten in place: `route` refuses `--yes` and `--exec`, starts nothing) | 1.0 |
| 1.4 — new: FakeJev probability vectors | `tests/common/mod.rs` | 1.2 |
| 1.5 — hunch-bx6 (filter and the scorer) | `src/cmd/filter.rs`, `tests/filter.rs` | 1.1, 1.2, 1.4 |
| 1.6 — new: `is` with several statements and `--context` | `src/cmd/is.rs`, `tests/is.rs` | 1.5 (uses the scorer) |
| 1.7 — new: records and save in `why`, records in `pick` | `src/cmd/why.rs`, `tests/why.rs`, then `src/cmd/pick.rs`, `tests/pick.rs` | 1.1, 1.2 |
| 1.8 contract — hunch-sb9 (skill) and new: 0.5.0 documents | `src/cmd/agent.rs` (with `init agents`), `tests/agent.rs`, `docs/ROBOT_MODE.md`, `README.md`, `docs/guide/*.md`, `PRIVACY.md`, `CHANGELOG.md`, `AGENTS.md`, `plugins/jevify/skills/jevify/SKILL.md`, `plugins/jevify/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`; last, alone: the version in `Cargo.toml`, `Cargo.lock`, `tests/release.rs` | 1.0; the version bump after 1.3, 1.6 and 1.7 |

1.1, 1.2 and 1.3 run in parallel; 1.8 runs beside everything. `src/args.rs` loses its caller in
1.3. That bead also removes the line `mod args;` from `src/lib.rs`, the one edit to a shared
file outside a skeleton; no other bead of the phase writes `src/lib.rs` after 1.0. The file
`src/args.rs` stays on disk, outside the module tree, until the owner allows its removal
(section 9).

Done when, in this repository, on the default backend:
```
cargo test 2>&1 | jevify filter -C2 'reports a failed assertion'                 # records, then: kept K of N, U unsure, full output: PATH
seq 1 10074 | jevify filter 'x' | head -3                                        # prints early, ends early, exit 0
git log --format=%s -500 | jevify filter -c -e 'fixes a bug' -e 'reverts a change'
fd -0 -e rs . src | jevify filter -0 --files 'spawns a child process'
jevify is 'a Rust crate manifest' 'names a binary target' --context Cargo.toml; echo $?
jevify route 'inspect Mach-O metadata'                                           # prints a tool, starts nothing
jevify run 'x'; echo $?                                                          # 2
jevify why -- true; echo $?                                                      # 2, the message shows the pipe form
```
and, as contract tests: `seq 1 20001 | jevify filter 'x'` is exit 6 with the
`--max-records 20001` line and no request; coloured `rg` output comes out byte-identical; 500
lines that differ only in a leading timestamp send one question; two runs over the same input
print the same path; an unwritable save directory gives `not saved` and `data.complete=false`;
a daily-quota 429 after the third batch prints a correct prefix and `answered 60 of N`.

### Phase 2 — `fill`: markers, `-`, `branch`, windows, `pick --from` (0.6.0)

Goal: an agent replaces "list, read, choose, act" with one call, for any list it can pipe, for
branches, and for an option that depends on text it has not read. The release holds exactly one
verb that starts a command.

In: `marker.rs` complete (`one` and `flag` included); `source.rs` with `-` and `branch`, both
tiers; the pool rule in `tournament.rs` and resolution through windows (2.1 steps 3 and 4,
hunch-zxz); `fill` with `--dry-run`, run, `-C`, `--candidates FILE`, `--context FILE`, several
markers, one request round for all context markers, the `flag` rule, the stdin rule, status
lines, signals, the branch identity check; the status-line drop in the output verbs;
`pick --from KIND`.
Out, on purpose: every other kind, recipes, `complete`.

| Bead | Writes | After |
|:---|:---|:---|
| 2.0 skeleton | shared files (`Fill` and its options, `Pick --from`, the error kinds of 2.1, `Cargo.toml` tokio `signal` and rustix `process`); stubs `src/marker.rs`, `src/source.rs`, `src/cmd/fill.rs` | Phase 1 |
| 2.1 — new: marker lexer, every form | `src/marker.rs` | 2.0 |
| 2.2 — hunch-q8p (kinds `-`, `branch`) and hunch-zxz (the pool rule) | `src/source.rs`, `src/tournament.rs`, `tests/tournament.rs` | 2.0 |
| 2.3 — new: `pick --from` | `src/cmd/pick.rs`, `tests/pick.rs` | 2.2 |
| 2.4 integration — hunch-bkb (fill core) | `src/cmd/fill.rs`, `tests/fill.rs`, `tests/fill_pipe.rs`, `src/input.rs` (`is_fill_status`, the drop) | 2.1, 2.2 |
| 2.5 contract | the contract files of 1.8 | 2.0; the version bump after 2.3 and 2.4 |

Done when, in this repository, on the default backend:
```
jevify fill --dry-run -- git switch '@{branch:the release automation work}'
git log --format='%H %s' -3000 | jevify fill --field 1 -- git show '@{-:made folder moves atomic}'   # status line: candidates 3000, windows 31
gh pr list --json number,title | jevify fill --dry-run --key number -- gh pr view '@{-:the Windows path fix}'
jevify fill -- git switch '@{branch:a branch that does not exist}'; echo $?      # 3, nothing ran
jevify fill --dry-run -- git switch '@{branch:x}' 2>&1 | jevify why; echo $?      # 3 upstream_not_run or a record of git, never a fill line
jevify --json fill -- true '@{-:x}' </dev/null; echo $?                          # 2, the message shows --dry-run
cat t.md | jevify fill -- true '@{-:x}' '@{flag:--draft:y}'; echo $?              # 2, stdin has two roles
jevify pick --from branch 'the release automation work'                          # one branch name
```
and, as contract tests: a branch that exists locally and on `origin` resolves; the
`gh issue create` example of the vision, under `--dry-run` with the cache off, sends exactly
one POST for its three markers (`meta.requests == 1`); a helper child receives the exact argv
in a run; an unsure `flag` is exit 3 `unsure_flag` and the sentinel file proves nothing ran;
`F + 1` lines on the classifier backend are `too_many_candidates` with no request; 250 lines
resolve in two rounds and the finals request holds three finalists of every window.

### Phase 3 — kinds: the recipe engine, then the coded kinds

Goal: a name for every list an agent meets, and one appended line for the next one.

The recipe engine and the shipped recipes of 2.2 (`pr`, `issue`, `ci-run`, `stash`, `process`,
`container`, `pod`) and the user's `kinds.jsonl`; `commit` with both tiers; `script`; `tool` for
`pick --from tool`, which calls the `route` code; `file` and `dir` with prefix scoping,
prefix-relative substitution, hidden paths with withheld excerpts and the `dev`/`ino` identity
check; `test` for cargo, pytest and go behind `--allow-collectors`.

| Bead | Writes | After |
|:---|:---|:---|
| 3.0 skeleton | shared files (`--allow-collectors`, `recipe_invalid`) | Phase 2 |
| 3.1 — new: recipe engine and shipped recipes | `src/source.rs`, `src/kinds.jsonl` | 3.0 |
| 3.2 — hunch-q8p, continued: `commit`, `script`, `tool` | `src/source.rs` | 3.1 (same file, serial) |
| 3.3 — new: path kinds | `src/source.rs` | 3.2 |
| 3.4 — new: `test` collectors | `src/source.rs` | 3.3 |
| 3.5 — tests and wiring | `src/cmd/fill.rs`, `tests/fill.rs`, `src/cmd/pick.rs`, `tests/pick.rs` | 3.4 |
| 3.6 contract | the contract files of 1.8; `docs/guide/kinds.md` (new: how to write and share a recipe) | 3.0 |

Done when `jevify fill --dry-run -- git show '@{commit:made folder moves atomic}'` prints a full
OID; `jevify fill --dry-run -- wc -l 'src/cmd/@{file:stages hunks}'` prints
`'wc' '-l' 'src/cmd/add.rs'`; a repository with a tracked `.npmrc` reports
`excerpts withheld: 1` and the request body captured by wiremock holds none of its content;
`jevify fill --dry-run -- cargo test '@{test:the retry backoff cap}'` is exit 6
`collector_not_allowed` and resolves with `--allow-collectors`; a line appended to a temporary
`kinds.jsonl` makes `'@{widget:…}'` resolve under `--allow-collectors`, and the same line under
the name `branch` is `recipe_invalid`; a `kinds.jsonl` in the working directory is never read;
`capabilities` lists every kind with its argv and class; collector tests with fake `git`, `gh`,
`cargo` and `pytest` first on the PATH assert argv, environment, directory, deadline, the killed
child, the bounded stderr tail, and "not logged in" as `enumerator_failed` with that text.

### Phase 4 — `label` (hunch-qxn)

`label a,b,c`: one Choice per record, the options plus an internal `NONE` id, 20 records per
request through the scorer of `filter.rs`. Labels are split on `,` and trimmed; an empty or
repeated label is exit 2; a label that holds a comma is given with `--label` repeated. Output is
`LABEL<TAB>RECORD` with the record's own terminator; an unsure record has the label `?`; JSON
records come out as `{"label":…,"p":…,"record":<the original value>}` lines.
`label --ordered low,mid,high` is the same Choice with an instruction that names the labels as
a scale in the given order, and the output adds the 1-based rank. `src/jev/mod.rs:18-28` has
Noul and Choice only; no Score wire type is added. `label` uses the scorer of `filter.rs`:
judge-once, batches, flow and the ceiling apply as they are.

| Bead | Writes | After |
|:---|:---|:---|
| 4.0 skeleton | shared files (`Label`) | Phase 1 |
| 4.1 — hunch-qxn | `src/cmd/label.rs`, `tests/label.rs` | 4.0 |
| 4.2 contract | the contract files of 1.8 | 4.0 |

Phase 4 needs Phase 1 only. It shares skeleton and contract files with Phases 2 and 3; those
beads run one after the other, the rest in parallel.

Done when `gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c` prints
a histogram, `cut -f2-` of the output is the input byte for byte, and an unsure record carries
the label `?`.

### Phase 5 — `complete`

The Cobra protocol behind `--allow-collectors`, after every other marker.
Writes: `src/source.rs`, `src/cmd/fill.rs`, `tests/fill.rs`, the contract files. Serial, after
Phase 3.
Done when a fake Cobra tool resolves `'--context=@{complete:the staging cluster}'` and a fake
tool without the directive line is `complete_unsupported`.

### After the language is stable

The language ships first; the evaluation of agent workflows follows it, on a surface that no
longer moves. Each item is one bead when that time comes.

- A local invocation log (time, verb, kind, exit code, inside or outside this repository), with
  its `PRIVACY.md` row, so that use is counted and not argued.
- A held-out set per verb and per kind, labelled by someone other than the author: accuracy,
  abstention rate, and the rate of a wrong pick that ran.
- Recall of `filter` at the unsure boundary on a hand-labelled collection.
- The window rule under load: how often the right candidate is outside the three finalists of
  its window.
- The rate of misquoted markers and of exit-2 recoveries in one turn.

### Later

Each item names what would make it worth building.

- A third round for `fill` above `F`: agents meet `too_many_candidates` on lists they cannot
  narrow.
- `--report FILE` for a run: a harness needs the resolution data of a real run in machine form.
  `run_cli` would write it after `meta` is complete; `fill` would only return the path.
- `JEVIFY_MAX_REQUESTS`, optional, no default: a user reports an unexpected bill or an exhausted
  daily quota.
- `--show-sent`: a user asks what a kind sends and `capabilities` does not answer it.
- A Score wire type: ordered labels through Choice prove unstable between neighbours.
- A marker form whose unsure answer leaves the flag out: callers show a flag whose omission is
  harmless and whose abstentions cost them turns.
- A setting for the ratio of 2.1: abstention reports show one constant does not fit both
  backends.
- Forwarding signals to a child's process group: a harness reports orphaned grandchildren after
  TERM.
- More shipped recipes: a user recipe is shared often enough to belong in `src/kinds.jsonl`.
- Normalising more than a leading timestamp before judge-once: logs show a repeat pattern that
  is safe to merge.
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
- Twin flags (section 1, rule 4). `pick -n N` and `why -n N` are "top N", as `head`; `filter -n`
  takes no value and numbers records, as `grep`. `fill` has `--dry-run` only, no `-n`. `-C` is a
  directory on `fill` and context on `why` and `filter`. Each help text names the twin.
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
  prefix; `W`, `W + 1`, `F` and `F + 1` candidates on each backend; with one window round 2 runs
  only when the ratio fails and sends at most 24 candidates; with several windows the finals
  hold three finalists of every window and the pool is not cut; two close rivals in one window
  both reach the finals and end as `ambiguous`; the right candidate in the last window wins; a
  branch with a remote twin resolves; several markers, one
  snapshot; stdin ownership (the child sees EOF with `@{-:…}`, inherits otherwise); `-C DIR`
  moves collectors and the child; `argv[0]` missing sends no request; a machine format without
  `--dry-run` is exit 2; every exit 2–6 ends with a `not run:` line, clap errors included;
  exit 0 and 7; `ran, signal N`; TERM reaches the child; a child that reads the terminal is not
  stopped (run under a pty when the test host has one, else reported as not run); a closed
  stderr pipe does not panic; a branch deleted between resolve and spawn is exit 6.
  Context markers: `one` + `flag` in one request; 21 context markers send two requests in one
  round; a `flag` no removes the element; a `flag` unsure is exit 3 and spawns nothing; a `flag`
  answer on the classifier backend; stdin with two roles is exit 2 before any read; an
  oversized context sends nothing; `--candidates` leaves stdin to the child.
- `tests/fill_pipe.rs`: the evidence line of a successful `fill` is not a record of `why`,
  `pick` or `is`; `upstream_not_run` behind clap text; a child line that starts with the
  reserved prefix is dropped and counted.
- Output verbs (Phase 1): record modes and their exit-2 combinations; CRLF, colour, NUL and
  non-UTF-8 records come out byte-identical; a JSON array element comes out as one JSON line;
  the saved path under `JEVIFY_NO_CACHE=1`; two saves of the same input; 20 concurrent saves;
  the failed-save path; `--no-save`; 64 MiB plus one byte. The scorer: judge-once for identical
  records and for a leading timestamp, and never for an inner number; `-e` halves the batch;
  flow (the first record is on stdout before the last request is answered, with a FakeJev that
  delays the last batch); a closed stdout is exit 0 and no panic; the ceiling and
  `--max-records`; the prefix after a daily-quota 429. `filter`: `-v`, `-c`, `-n`, `-A/-B/-C`
  with the `--` separator, unsure kept in both directions, `--strict`. The algebra, as one
  test: `filter A | filter B` equals `filter B | filter A`, and `label … | cut -f2-` equals the
  input. `is --context FILE` equals `is < FILE`.
- Recipes (Phase 3): every shipped line parses; an unknown field, a bad line and a shadowed
  kind are `recipe_invalid` with the line number; `more` receives the handle as one argv
  element, a handle with spaces included; the working directory is never searched.
- Collector tests put fake `git`, `gh`, `cargo` executables first on the PATH and assert argv,
  directory, deadline, the kill, the stderr tail and the error kind.
- Live tests (ignored by default) for one `branch` and one `one` resolution per backend. Without
  a key they are reported as not run, never as passed.

## 6. The README, the skill and the `AGENTS.md` block

Three documents teach one language to three readers. They share their order (the two sides, the
verbs with their twins, the marker, the exit codes), their examples and their words. Bead 1.8
rewrites all three from an empty page; each later contract bead adds what its phase delivers.
None of them names a verb, a flag or a kind the installed release lacks.

### The README, for a person

`README.md` today has a section per verb in the order they were built (`pick`, `why`, `is`,
`run`, `add`, `sort`), a "Numbers" section, and `jevify run` in its first screen. The rewrite:

| Section | Content |
|:---|:---|
| Title and one sentence | "jevify gives command-line tools an understanding of meaning." Then the two-sides figure of the vision. |
| Thirty seconds | Install, no key needed, and three commands that work in any repository: a `filter` on a log, an `is` in an `&&` chain, a `fill --dry-run` on a branch. |
| The output side | The verb table with the twin column; the six lines of the algebra from the vision; `why` for a log, `filter` for records; the way back (`full output: PATH`). |
| The input side | `fill`, the marker in one paragraph, the three families, `--dry-run`, all-or-nothing, the status lines. |
| Kinds | The kind table; a recipe in one line; where `kinds.jsonl` lives. |
| Exit codes | The table of `AGENTS.md`, and "write the condition so that yes means act". |
| For agents | The skill install line, `jevify init agents`, the permission rule of this section. |
| What it is not | The four lines of the vision. |
| `add`, `sort`, `route` | Short; they keep their guide pages. |
| Privacy, limits, docs, credits, license | As today, with the two stores of section 7 and the one ceiling. |

The "Numbers" section leaves the README. Measurements live in `docs/guide/how-it-works.md`
next to their date and conditions, and return to the README when the evaluation of "After the
language is stable" has produced them. `docs/guide/verbs.md` follows the README's order;
`docs/guide/kinds.md` is new in Phase 3.

### The skill, for a Claude agent

The file is `plugins/jevify/skills/jevify/SKILL.md`. It teaches `jevify run`, `why -- CMD`,
a shell loop over `is`, and `git show "$(… | jevify pick …)"` today; every one of those is a
form the language replaces. `plugin.json` and `marketplace.json` carry the same version as the
crate. The frontmatter description leads with the two situations an agent meets most: an output
too long to read, and a value it can describe and cannot spell.

### The `AGENTS.md` block, for every other agent

`jevify init agents` prints a block of at most 25 lines for `AGENTS.md`, `CLAUDE.md` or any
instruction file: when to reach for jevify (the triggers below, one line each), the five output
verbs with their twins, one `fill` example with the quoting habit, the exit codes, and
`jevify capabilities --json` as the source of truth. It is generated from the same table as
`capabilities`, so it never names what the build lacks. `init zsh` and `init bash` stay.

Triggers, one line each: about to list branches, commits, tests, PRs or runs only to choose one →
`fill`; can describe it, cannot spell it, and a tool can list it → pipe the list into
`fill … '@{-:…}'`; want the value and not the run → `pick --from KIND`; an option depends on a
ticket, diff or log you have not read → `@{one:…}` / `@{flag:…}` with the text on stdin; a
failed build or test with more than about 50 lines → `2>&1 | jevify why`; many records and one
yes/no question → `filter`; many files and one question → `fd -0 | jevify filter -0 --files`;
every record needs a bucket → `label`; the next step depends on a fact about some text →
`is … &&`; waiting for a state you can describe → `until … | jevify is '…'; do sleep 5; done`.
Skip jevify for a name already seen, a literal search, counts and dates, and untrusted text.

Habits: one jevify process per question, never a shell loop that starts one per record; cheap
tools first (`grep -v`, `jq`, `head`), jevify last; write a condition so that yes means act;
a printed handle is for reading, and as an argument it goes through `fill`, never through
`"$(jevify pick …)"`, where an abstention becomes an empty argument; under `git bisect run`,
map exit 3 to 125 (`case $? in 0) exit 0;; 1) exit 1;; *) exit 125;; esac`), so an unsure answer
skips the commit; the whole marker argument in single quotes with no quotes inside; an
apostrophe is `'\''`; run `fill` directly and use `--dry-run` to look, never `eval` its output;
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
| exit 3 `unsure_flag` | read the probability; write the flag or drop the marker, then run again |
| exit 2, stdin has two roles | move one input to `--candidates FILE` or `--context FILE` |
| `too_many_records` | narrow with `grep` or `head` first, or copy the `--max-records` command from the message |
| exit 4 with `answered N of M` | the printed records are a correct prefix; continue later from record N + 1 |
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

A judgment is almost free and the keyless backend is free, so nothing in jevify rations
requests. What is bounded is time: two rounds per verb, every request of a round in flight at
once under the client semaphore (4 on classifier.dev, which is shared per IP; 8 on TypeSafe;
`JEVIFY_CONCURRENCY` changes it), a deadline per collector, the ceiling of 20,000 records, and
the 64 MiB input. A `fill` over `F` candidates is 33 windows and one finals request, about nine
waves on the keyless backend; `filter` and `label` print `N records, M distinct, R requests`
before the first request and let their output flow. A daily-quota 429 is never retried; other
429s keep the retry policy of `src/jev/client.rs:418-422`.

Two stores exist and the documents keep them apart: the answer cache (redacted requests and
responses, seven days, `--no-cache`) and the saved inputs (raw, never pruned by jevify,
`--no-save`).

## 8. Risks

1. **A wrong pick runs.** Guards: the `NONE` option, the `any` Noul, the ratio against both the
   runner-up and `NONE`, finals that hold every window's finalists in one comparison, the
   tier-two round, one candidate per branch, an unsure `flag` abstains, the identity re-check,
   all-or-nothing, the evidence line the agent reads. No effect classes; the harness permission
   system owns that decision. A window can hold more than three candidates that beat the right
   one; then the right one misses the finals, and the finals' `NONE` and `any` are the guard.
2. **Agents misquote the marker.** The lexer turns every damaged marker it can see into exit 2
   with the corrected spelling, and zero markers is an error. A shell can still leave a valid
   marker with a damaged description (`$x` expanded inside double quotes); the lexer cannot see
   that, and the skill's single-quote habit is the guard.
3. **Collectors are slow or run code.** The inert/executing split, `--allow-collectors`, a
   deadline per kind with kill and reap, the collector argv in `capabilities`. `--dry-run` runs
   inert collectors and says so.
4. **A long list is slow, not refused.** `F` candidates on the keyless backend are 34 requests
   at 4 in flight. The status line shows the window count, the skill teaches "cheap tools
   first", and a literal prefix or a piped list makes the same call fast. Above `F` the message
   teaches the prefix and the pipe, and ordered kinds keep the newest `F`.
5. **The keyless backend** takes 99 options and 20 questions per request, is shared per IP, and
   has a daily quota. Per-record verbs state their request count before they start, flow their
   output, and leave a correct prefix when the quota ends a run.
6. **Saved inputs hold secrets and grow.** 0600 in a 0700 directory, named in `PRIVACY.md`,
   `--no-save`; jevify never deletes them.
7. **A child prints the reserved prefix.** The line is dropped and counted in
   `data.fill_lines_dropped`; the saved input keeps it.
8. **A grandchild outlives a TERM.** jevify signals the child only (Later).
9. **A user recipe runs a command.** It lives only in the user's configuration directory, never
   in a repository, needs `--allow-collectors`, cannot shadow a shipped kind, and `capabilities`
   prints its argv.
10. **Batch neighbours share a state.** Twenty records are judged in one request, so a record's
    answer can lean on its neighbours. Each question names its record by id and the instruction
    says to judge it alone; the evaluation after the language is stable measures the effect.

## 9. Owner decisions

1. `git mv src/cmd/run.rs src/cmd/route.rs`, `tests/run_route.rs` → `tests/route.rs`, and the
   removal of `src/args.rs` and `tests/run_args.rs`. Without permission the verb is renamed, the
   files keep their names, `src/args.rs` stays on disk outside the module tree, and
   `tests/run_args.rs` holds the "route starts nothing" tests.
2. An unsure `flag` abstains and nothing runs (2.1 step 5). The cost: the vision's
   three-argument example stops whenever one Noul lands in the band, and the caller spends one
   turn to write or drop that flag. The alternative, leave it out and run, lets doubt choose an
   act.
3. `fill` resolves through windows up to `F` candidates (3,267 keyless, 13,200 with a key) in two
   rounds, and the pool of finalists is never cut; ordered kinds above `F` keep the newest `F`
   (2.1 steps 3 and 4, hunch-zxz).
4. Saved inputs are content-addressed and never pruned by jevify (2.4).
5. A machine format on a `fill` run is exit 2; there is no `--report FILE` (2.1).
6. Limits bound time, not requests: one ceiling of 20,000 records in every verb, no refusal
   below it, no request budget (sections 1 and 7).
7. User recipes are read from the user's configuration directory only, as JSON lines, with no
   new dependency (2.2).
8. The output side ships first, as 0.5.0 with the grammar cutover; `fill` follows as 0.6.0.
   Evaluation follows the language (section 3, "After the language is stable").

## 10. Beads

| Phase | Existing beads | New beads |
|:---|:---|:---|
| 0 | hunch-ed1 (extended: backend bound to host, no redirects) | — |
| 1 | hunch-bx6 (filter and the scorer, bead 1.5), hunch-sb9 (skill, bead 1.8; its text names `skills/`, the path is `plugins/jevify/skills/jevify/SKILL.md`) | language skeleton and grammar cutover (1.0); byte records, every mode (1.1); saved input (1.2); route is print-only (1.3); FakeJev probability vectors (1.4); `is` with several statements and `--context` (1.6); records and save in `why` and `pick` (1.7); 0.5.0 documents, README and `init agents` (1.8) |
| 2 | hunch-bkb (fill core, bead 2.4), hunch-q8p (kinds `-`, `branch`, bead 2.2), hunch-zxz (the pool rule, bead 2.2) | fill skeleton (2.0); marker lexer, every form (2.1); `pick --from` (2.3); contract (2.5) |
| 3 | hunch-q8p continued (`commit`, `script`, `tool`) | recipe engine and shipped recipes; path kinds; `test` collectors and `--allow-collectors`; tests and wiring; contract with `docs/guide/kinds.md` |
| 4 | hunch-qxn (label) | contract |
| 5 | — | `complete` |
| After, Later | — | one bead per item when its time or its trigger comes |

Close as superseded: hunch-k6s, hunch-x36, hunch-4z4 (replaced by this plan's epic, hunch-vq2).
Close as obsolete with the argument pass, in Phase 1 (bead 1.3): hunch-gjf, hunch-eil, hunch-u07,
hunch-zss.

# jevify: the two sides of a command — implementation plan

This plan implements `docs/VISION.md`. The vision decides the product; this document decides the
code. Where they disagree, the vision wins and this file is corrected.

Bead IDs keep the `hunch-` prefix. A bead that does not exist yet is written `new: <title>`.
Code references (`file:line`) point at the tree at version 0.4.0.

## 1. The contract in one page

```
jevify VERB [options] ['text'…] [-- COMMAND ARGS…]
```

| Verb | Side | Reads | Starts a command | Exit codes |
|:---|:---|:---|:---|:---|
| `fill` | input | argv after `--`; stdin or a file for `-`, `one`, `flag` | yes, the only one: it becomes the command | 2–6 nothing ran; otherwise the command's own exit code |
| `pick` | output | stdin, or `--from KIND` | no | 0 found, 3 abstain |
| `route` | output | the PATH inventory (`pick --from tool`) | no | 0 found, 3 abstain |
| `why` | output | stdin | no | 0 found, 3 abstain |
| `filter` | output | stdin | no | 0 kept some, 1 kept none, 3 every record unsure |
| `label` | output | stdin | no | 0 labelled, 3 every record unsure |
| `is` | output | stdin, or `--context FILE` | no | 0 all yes, 1 one no, 3 otherwise |
| `add`, `sort` | acting verbs, unchanged | — | git apply / rename only | unchanged |

Five rules that every section applies:

1. **Selection.** Every record that `pick`, `filter` and `label` print is a record of the input,
   byte for byte. `why` is exempt: it points and does not reduce, so it prints each line with
   its number and its context, as today (`src/cmd/why.rs:197-208`), and it takes no split
   option (`-0`, `--para`, `--files`). Every value `fill` substitutes is a handle that a lister
   printed or the caller wrote.
2. **Time is the cost.** A judgment is almost free; a round trip is not. A verb asks everything
   it can in one round, in parallel, and uses at most two rounds.
3. **Asymmetric doubt.** `filter` keeps what it is unsure about: a dropped record costs the
   whole input. `fill` and `pick` abstain: a wrong act costs more than no act.
4. **Use the tools that exist.** jevify does not do what `jq`, `cut`, `grep`, `head`, `tee` and
   `cd` do. It reads lines and gives lines back, so they all compose with it.
5. **One role for stdin.** A call never reads stdin for two purposes.

Removed, with no shim, in Phase 1 (release 0.5.0): `why -- CMD`, the verb name `run`,
`run --yes`, `run --exec`, `run --dry-run`, `run --no-args`, the `run` argument pass
(`src/args.rs`, `manpage::parse_flags`), `pick --files DIR`, and the global short flag `-v`.
`init` stays: its `,` alias and its `command_not_found` hook call `jevify route`, which only
prints (`src/cmd/agent.rs:188-203` call `jevify run` today).

- **`-v` is inversion.** `filter -v` keeps the records where the statement is false, as
  `grep -v`. The global short flag `-v` of `--verbose` (`src/cli.rs:36`) leaves; `--verbose`
  stays. `tests/cli_basics.rs:52` passes `-v` for verbosity today and passes `--verbose` after
  bead 1.0; a second test proves `is 'x' -v` is exit 2 and `filter -v` inverts.
- **`--files` is a boolean** on `pick`, `filter` and `label`: the records on stdin are paths.
  `pick --files DIR` (`Option<PathBuf>`, `src/cli.rs:66-67`) leaves with no shim; the form is
  `git ls-files | jevify pick --files 'q'` (or `fd`). The two-stage engine of `pick --files`
  stays (path names first, excerpts for the finalists); only the source of the paths changes.
  `pick --index` stays as it is. `pick --from file` arrives in Phase 3.
- **`is` with one statement prints nothing on stdout**, as today. With two or more it prints
  one verdict line per statement. `tests/is.rs` states both.

Marker grammar:

```
marker  := "@{" KIND ":" BODY "}"
KIND    := "-" | [a-z][a-z-]*            registered kinds only
BODY    := bytes up to the first unescaped "}"; "\}" is a literal "}"
one     := "@{one:" OPT ("|" OPT)* ":" QUESTION "}"      escapes \: \| \}
flag    := "@{flag:" FLAGTEXT ":" QUESTION "}"           whole argv element; FLAGTEXT starts with "-"
escape  := "@@{"  →  literal "@{"
```

- One wrapping pair of `'…'` or `"…"` around BODY is dropped. A newline in BODY becomes a space.
- Escapes are decoded once. Substituted text is never lexed again, so a handle that holds `@{`
  stays literal.
- Exit 2, nothing runs: unknown kind (the error names the nearest kind and lists the kinds),
  `@{word:` with no closing `}`, empty BODY, BODY that is not UTF-8, `flag` embedded in a larger
  argument, zero markers, a `one` with fewer than two options or a repeated option, a marker in
  `argv[0]`, and stdin with two roles (a `-` marker without `--candidates FILE` next to a `one`
  or `flag` marker without `--context FILE`).
- Every exit-2 message is one line that an agent can act on in one turn: the corrected argument
  in single quotes. A command given as one string (`argv[0]` holds a space) gets "pass the
  command as separate arguments".
- `@{` followed by anything that is not `KIND:` is literal (`@{u}`, `HEAD@{2}`, `@{1 day ago}`).
  `KIND:` is a syntactic test: the Python format string `{user}@{host:>8}` holds `@{host:`,
  which is a marker with an unknown kind. It is exit 2, not a literal, and the message shows
  the `@@{` spelling: `'{user}@@{host:>8}'`.
- Every other byte of every argument passes through unchanged. Argv is `OsString` end to end.
- After `--` every token belongs to the command, a second `--` included.

## 2. Architecture

New files:

| File | Contents | Tests |
|:---|:---|:---|
| `src/marker.rs` | Pure lexer: `parse(&[OsString]) -> Result<Vec<Arg>, MarkerError>`; `substitute`. No I/O. | inline table tests |
| `src/records.rs` | The record model of `pick`, `filter`, `label` and of `@{-:…}`, over raw bytes. `Record { handle, evidence: String, raw: Range<usize> }` indexes one input buffer. It also holds the one withholding policy for excerpts (2.2), which `--files` applies on every verb. | inline |
| `src/source.rs` | `enumerate(kind, scope, limit, env) -> Listing { records, total, omitted }`: the coded kinds and the recipe engine (2.2). Listers run an argv, never a shell, under one deadline. `env` is the injected environment: `PATH`, the configuration directory, the deadline. | inline + `tests/fill.rs`, `tests/pick.rs` |
| `src/kinds.jsonl` | The shipped recipes, one JSON object per line, compiled in with `include_str!`. | the recipe table test |
| `src/cmd/fill.rs` | lex → read inputs → enumerate → resolve → substitute → print or `exec`. | `tests/fill.rs` |
| `src/cmd/filter.rs`, `src/cmd/label.rs` | Per-record verbs; the scorer of 2.3 lives in `filter.rs`. | `tests/filter.rs`, `tests/label.rs` |
| `src/save.rs` | The content-addressed saved input of 2.4. | inline |

Changed in place:

| File | Change |
|:---|:---|
| `src/jev/client.rs`, `src/jev/classifier.rs` | `Client::ask_each(records, questions)`, the per-record request of 2.3 |
| `src/cli.rs` | `Fill`, `Route`, `Why` without `cmd`, `Filter`, `Label`, `Pick --from`, `Is` with several statements and `--context`; `--files` as a boolean; `Init` takes `zsh`, `bash` or `agents` (`:154-161`); the global `-v` leaves (`:36`) |
| `src/lib.rs` | `VERBS` (`:23`), `QUICK_START` (`:37-51`), `command_name` (`:128`); the clap error path reads `args_os` and stops at `--` (`:63` uses `env::args()`); `human` written as bytes (`:151-158`); the `route` dispatch arm calls `cmd::run::run(ctx, intent, machine)` (`:257-275` builds `RunFlags` today) |
| `src/cmd/mod.rs` | `Outcome` changes once, in bead 1.0: `human: Vec<u8>` (`:15` is `String`) and `exec: Option<Exec>` with `struct Exec { argv: Vec<OsString>, stdin_null: bool }`, `None` at every construction site. Bead 2.0 adds no field; it only makes `run_cli` act on `exec`, after it has written everything else |
| `src/exit.rs` | one variant `JevifyError::Kinded { kind, exit, message, hint, example }` for the new kinds |
| `src/input.rs` | `read_stdin_bytes()` with the same 64 MiB cap and terminal check |
| `src/output.rs` | `shell_quote(&[OsString]) -> Vec<u8>`: `shell_display` of `run.rs:312`, moved and written over bytes |
| `src/tournament.rs` | `window` becomes `pub(crate)` (`:55`); `decide(&Ranking)`; the pool rule of 2.1 for every caller of `rank` (`:126-127`) |
| `src/cmd/why.rs` | delete `capture` and the `cmd` argument (`:59-`); the saved input; the output format stays |
| `src/cmd/run.rs` | becomes `route`: bead 1.0 sets the signature `run(ctx, intent, machine)` and removes `RunFlags`; bead 1.4 deletes execution, the confirmation, the argument pass |
| `src/cmd/pick.rs`, `src/cmd/is.rs` | byte records, `--files` from stdin, `--from KIND`; several statements, `--context FILE` |
| `src/cmd/agent.rs` | Phase 0: the `health` client follows no redirect (`:127`); capabilities, robot-docs, `init agents` |
| `src/config.rs` | Phase 0; the save directory; `JEVIFY_CONFIG_DIR` next to `JEVIFY_CACHE_DIR` (`:99`) in Phase 3 |
| `tests/common/mod.rs` | `FakeJev` answers with a probability vector per question |
| Documents | section 6 |

No new dependency. `fill` replaces itself with the command (`CommandExt::exec`, std, safe), so
there is no child to wait for, no signal to forward and no tokio `signal` feature.

### 2.1 `fill`

`W` is the window of the backend: 200 candidates on TypeSafe, 99 on classifier.dev
(`Backend::window`, `src/config.rs:34`). `F = W × (W / 3)`: 13,200 and 3,267.

**One pool rule for every caller of `tournament::rank`** (`fill`, stdin `pick`, `pick --from`,
`why`). The finalists per window are `n` = 3 when `3 × windows ≤ W`, else 2 when
`2 × windows ≤ W`, else 1: always by rank inside a window, never by a probability from another
request. The capacity is `W × W` candidates: 9,801 keyless; on TypeSafe the ceiling of 20,000
is reached first.

| Caller | `n` | Limit | Above the limit |
|:---|:---|:---|:---|
| `fill` | 3 only | `F = W × (W / 3)` | an ordered kind keeps its newest `F`; any other kind is exit 6 `too_many` |
| `pick`, `pick --from` | the full rule | `W × W`, and 20,000 (`MAX_LINES`, `src/cmd/pick.rs:9`) | an ordered list keeps its newest `W × W` and the status line says so; any other list is exit 6 `too_many` (narrow with `grep`, `head` or a prefix) |
| `why` | the full rule | `MAX_KEEP = 4000` (`src/cmd/why.rs:14`): 41 windows keyless, `n = 2` | `why` keeps at most `MAX_KEEP` lines by its own code, as today |

The status line names `n` when it is not 3: `finalists per window: 2`. `tests/tournament.rs`
proves it on the classifier backend with 4,000 items (41 windows, `n = 2`, 82 finalists in one
finals request) and with 9,802 items (`too_many`, no request).

1. **Lex and validate** the whole argv and the options before any I/O. A machine format without
   `--dry-run` is exit 2. A literal `argv[0]` that is not on the PATH is exit 6 `cannot_run`.
2. **Read inputs.** `-` reads `--candidates FILE` or stdin. `one` and `flag` read
   `--context FILE` or stdin. stdin is read once, on the blocking pool, up to 64 MiB. A required
   stdin that is a terminal is exit 6 `stdin_is_tty`. A context above the evidence budget is
   exit 3 `insufficient_evidence` with no request, as `is` does (`src/cmd/is.rs:21-31`).
3. **Enumerate** each distinct `(kind, prefix)` once, in parallel. A lister that fails or passes
   its deadline is killed, and the call is exit 6 `lister_failed` with a bounded tail of the
   tool's stderr. Then, per marker:
   - A handle that cannot be substituted is dropped and counted in `omitted`: one that holds a
     newline, a carriage return or a NUL, and one that starts with `-` when the marker opens
     its argument and the kind is not a path.
   - Two candidates with the same handle and evidence are one. Candidates with the same
     evidence and different handles stay separate and end as `ambiguous`; the status line says
     `N candidates share the same evidence`.
   - Zero candidates is exit 3 `no_match` with no request.
   - More than `F` candidates: an ordered kind keeps the newest `F` and says so. Any other
     kind is exit 6 `too_many` with the count and the two ways to narrow: a literal path
     prefix, or a narrower list piped into `'@{-:…}'`.
4. **Resolve**, at most two rounds. Every request of a round is in flight at the same time.
   - Round 1. Each listing marker sends one `tournament::window` per `W` candidates (one Choice
     with `NONE`, one `any` Noul). All `one` and `flag` markers go out over the context: one
     Choice per `one`, one Noul per `flag`, 20 questions per request
     (`MAX_DIMENSIONS`, `src/jev/classifier.rs:26`).
   - Round 2, one request per listing marker. With several windows it is the finals: the three
     best of each window **by rank** (`n = 3`, the only value `fill` uses), all of them, with
     tier-two evidence for the first 24 (`MAX_FINALISTS`). A probability is never compared with
     one from another request (`src/tournament.rs:126-127` does that today and keeps 24). With
     one window, round 2 runs only when the ratio failed and the kind has tier-two evidence.
   - A `one` marker holds at most `W` options, because `NONE` takes one slot of the Choice:
     more is exit 2 with the count. The lexer has no backend, so `fill` checks it
     (`tests/fill.rs`: `W` and `W + 1` options on each backend).
5. **Decide** per marker.
   - Listing kinds: found when `any ≥ threshold` and
     `p(best) ≥ 2 × max(p(second), p(NONE))`. A ratio holds for 3 options and for 200. Equal
     scores abstain. The factor is a constant in `tournament.rs`.
   - `one`: the same ratio over the options and `NONE`.
   - `flag`: `is::band_verdict` (`src/cmd/is.rs:54-62`). Yes keeps the argument, no leaves it
     out, **unsure abstains**: `not run: arg 5 flag --draft: unsure 0.48; write --draft or drop
     the marker`.
   - **The model guard.** classifier.dev names the model that answered, and answers with
     another model when Jev is not available. `fill` runs a command only on an answer from
     Jev; any other model is exit 4 `unavailable`, and the line names the model. Output verbs
     answer and name the model in their status line.
   - Reasons for exit 3: `no_match`, `ambiguous`, `unsure_flag`, `insufficient_evidence`.
6. **All or nothing.** Any abstention means nothing runs: exit 3, empty stdout, one line per
   failed marker:
   `jevify fill: not run: arg 3 branch: ambiguous; closest: tp/auth (0.41), tp/auth-v2 (0.38)`.
   With several failed markers, the envelope's `error.kind` and `data.reason` are those of the
   first failed marker in argv order, and `data.markers[]` lists every marker with its own
   reason (`tests/fill.rs`: a `no_match` at arg 2 and an `unsure_flag` at arg 4 give
   `no_match`, and both appear in `data.markers[]`).
7. **Substitute.** A handle is an `OsString` and is substituted as bytes, so a path that is not
   UTF-8 reaches the command unchanged. A path handle is relative to the prefix it was listed under, so
   `'src/@{file:…}'` becomes `src/cmd/add.rs`. A path handle that opens its argument and
   starts with `-` gets `./`. A `flag` that is left out removes its whole argv element.
8. **`--dry-run`:** print the quoted command on stdout, one line, through `output::shell_quote`;
   exit 0. Quoting works on bytes, so an argument that is not UTF-8 prints and reads back
   exactly. With a machine format the envelope carries `data.argv`; an argv that is not UTF-8
   is exit 6 `cannot_run` there, because JSON cannot hold it.
9. **Run.** Print the evidence lines and `jevify fill: exec <quoted command>` on stderr, flush,
   then `exec` the argv. stdin is `/dev/null` when step 2 consumed it, inherited otherwise.
   From here the process is the command: its output, its signals, its terminal, its exit code.
   A failed `exec` is exit 6 `cannot_run`.
   - The call is `std::os::unix::process::CommandExt::exec` on a
     `Command::new(&argv[0]).args(&argv[1..])`, with `.stdin(Stdio::null())` when stdin was
     consumed. It is a safe function, so `#![deny(unsafe_code)]` holds. It returns only on
     failure, with the `io::Error`: `NotFound` and `PermissionDenied` become `cannot_run` with
     the path in the message.
   - `fill` returns `Outcome.exec = Some(Exec { argv, stdin_null })` to `run_cli` (the field
     exists since bead 1.0, `None` everywhere else), and `run_cli` calls `exec` as its last act,
     after `meta` is computed and the status lines are written. No destructor of jevify runs
     after `exec`, so everything that must reach the disk is written before it: the answer
     cache entries of this call (`DiskCache::put` writes through a rename and holds no buffer)
     and the stderr lines (stderr is unbuffered).
   - The tokio runtime is current-thread. At `exec` no request is in flight, because every
     marker is resolved; the blocking pool threads vanish with the process image, and none of
     them holds work.
   - The environment and the working directory pass through unchanged. jevify sets no variable
     for the command.
   - Tests use a POSIX sh helper (`tests/bin/argv.sh`, new), run as
     `sh tests/bin/argv.sh <exit-code> args…`: it prints its arguments NUL-separated, prints
     `stdin:eof` or `stdin:data`, and exits with the given code. It proves the exact argv, the
     stdin rule and the exit code that comes through. It is no Cargo target: `cargo metadata`
     shows one bin target, `jevify`.
   - After an `exec` line, the exit code belongs to the command. Without one, the last line is
     `jevify fill: not run: <reason>` and the code is 2 to 6.
   - `fill -q` prints only `not run:` lines. `jevify fill -q -- CMD 2>&1 | jevify why` then
     reads the command's output alone, and a `not run:` line is the true cause, which `why`
     may point at. Output verbs have no special case for `fill`.

Status lines, all on stderr, all prefixed `jevify fill:`:

```
jevify fill: branch tp/auth-refactor 0.91 (next 0.04, none 0.02) "move session check into middleware", 3 days; candidates 41
jevify fill: commit 3f9c2ab 0.88 (next 0.06, none 0.01) "make folder moves atomic"; candidates 1432, windows 15
jevify fill: flag --draft: no 0.07, left out
jevify fill: exec 'git' 'switch' 'tp/auth-refactor'      | not run: arg 3 branch: ambiguous; …
```

New stable error kinds, six: `stdin_is_tty`, `lister_failed`, `too_many`, `cannot_run`,
`recipe_invalid` (all exit 6), and the abstention reason `unsure_flag` (exit 3). The message
line carries the detail; a kind names only what an agent does next.

### 2.2 Kinds

Coded kinds, the ones that need logic:

| Kind | Lister (argv) | Handle | Tier 1 evidence | Tier 2 |
|:---|:---|:---|:---|:---|
| `-` | stdin / `--candidates FILE` via `records.rs` | `--key` / `--field N` / whole record | the whole record | — |
| `branch` | `git for-each-ref --sort=-committerdate --format=… refs/heads refs/remotes` | local short name; `origin/x` for a remote-only ref | name, tip subject, age computed by code | + last 5 subjects, changed top-level paths |
| `commit` | `git log -n <limit> --format=… HEAD`, NUL-delimited; the total from `git rev-list --count HEAD` | full OID | subject | + body, changed paths |
| `file`, `dir` | `git ls-files -co --exclude-standard -z`; outside a work tree a no-follow walk | path relative to the prefix | path | + first lines (`sort::excerpt`, behind the withholding function of `src/records.rs`) |
| `tool` | `inventory::load` (exists) | name | name + whatis line | — |

A recipe kind is the `-` form with a name, one JSON object on one line:

| Field | Meaning | Default |
|:---|:---|:---|
| `kind` | the name in the marker | required |
| `list` | the lister argv | required |
| `field` / `key` | the handle: the N-th field of a line, or a key of a JSON value | the whole line |
| `ordered` | the lister prints newest first, so a list above `F` keeps its head | `false` |

```
{"kind":"pr","list":["gh","pr","list","--state","all","--limit","1000","--json","number,title,state,headRefName"],"key":"number","ordered":true}
{"kind":"issue","list":["gh","issue","list","--state","all","--limit","1000","--json","number,title,state"],"key":"number","ordered":true}
{"kind":"ci-run","list":["gh","run","list","--limit","1000","--json","databaseId,displayTitle,status,conclusion,headBranch"],"key":"databaseId","ordered":true}
{"kind":"stash","list":["git","stash","list","--format=%gd%x09%gs"],"field":1,"ordered":true}
{"kind":"process","list":["ps","-axo","pid=,comm=,args="],"field":1}
{"kind":"container","list":["docker","ps","--format","{{.Names}}\t{{.Image}}\t{{.Status}}"],"field":1}
{"kind":"pod","list":["kubectl","get","pods","--no-headers"],"field":1}
```

- The evidence of a recipe candidate is its whole record. The lister's own flags choose the
  fields (`--json number,title`, `--format`), so a recipe has no evidence option.
- Shipped recipes live in `src/kinds.jsonl`. User recipes live in `kinds.jsonl` in the user's
  configuration directory: `JEVIFY_CONFIG_DIR`, read in `src/config.rs` next to
  `JEVIFY_CACHE_DIR` (`:99`), or `directories::ProjectDirs` (already a dependency). The
  sanitized test command of `tests/common/mod.rs` removes `JEVIFY_CONFIG_DIR`, as it does for
  the other variables, so a developer's own recipes never reach a test. A user recipe cannot
  replace a coded kind or a shipped recipe.
- The user's `kinds.jsonl` is read only when a marker names a kind that is neither coded nor
  shipped. Then every line must parse: any bad line is exit 6 `recipe_invalid` with its line
  number, whichever kind was asked for. A line of invalid JSON names no kind, so the file is
  valid as a whole or not at all.
- jevify reads no recipe from the working directory. A cloned repository never adds a command
  that `fill` runs. A user recipe is the user's own command, as an alias is.
- The engine is the `-` path: run `list`, hand the bytes to `records.rs`, get a `Listing`.
  A recipe is `#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Recipe { kind,
  list: Vec<String>, field: Option<usize>, key: Option<String>, ordered: bool }`. With `key`
  the output is read as JSON lines or one array, through `serde_json`; with `field` or
  neither, as lines. `field` and `key` together, an empty `list`, and a `kind` that does not
  match `[a-z][a-z-]*` are `recipe_invalid`.
- **The lister runner.** A `std::process::Command` with piped stdout and stderr, two detached
  reader threads that send bytes over a channel, and a `try_wait` poll every 20 ms until the
  deadline. On the deadline: `kill`, `wait`, then at most 200 ms for the readers, then
  `lister_failed`. A descendant that still holds the pipe cannot block jevify (the
  `DAEMON_GRACE` pattern of `src/cmd/why.rs:17`). Output above 64 MiB stops the read and is
  `lister_failed`. No process group, no new `rustix` feature. A `tokio::time::timeout` around
  a blocking task is not the mechanism: a started blocking task cannot be aborted. Inline
  tests: a lister that sleeps, a lister whose child keeps the pipe open, an oversized output.
- **The environment is injected.** `source::enumerate` and the runner take `PATH`, the
  configuration directory and the deadline as parameters, and the runner passes them through
  `Command::env`. No `set_var` in `src/` (`#![deny(unsafe_code)]`). Inline tests pass a
  temporary `PATH`; the cases that depend on the process environment (`tool` through
  `inventory::load`, user recipes, fake `git` and `gh`) are process-level tests in
  `tests/fill.rs` and `tests/pick.rs` through `assert_cmd`'s `.env`.
- **Listers take a `limit` from the caller**: `F` for `fill`, the capacity of 2.1 for
  `pick --from`. `commit` lists `git log -n <limit>` and reads the total from
  `git rev-list --count HEAD`, so `candidates N of M, newest first` is exact.

Rules for every kind:

- One deadline: 20 s. Every lister runs with stdin at `/dev/null`, `GH_PROMPT_DISABLED=1`,
  `GIT_TERMINAL_PROMPT=0` and `NO_COLOR=1`. A tool that is not on the PATH, not logged in or
  rate-limited is `lister_failed` with the tool's own text, never an empty listing.
- `branch` lists one candidate per branch: a remote ref whose local branch exists is folded
  into it, and symbolic refs are skipped. Twins split the Choice mass and abstain
  (`src/cmd/pick.rs:44-46` states the same for repeated lines).
- A literal prefix narrows path kinds. The prefix is the literal text before the marker when it
  ends with `/`: the whole text if it names a directory, else the part after the first `=`
  (`'--config=conf/@{file:…}'`). Neither names a directory: `lister_failed`.
- Candidates come from the directory jevify runs in. `cd DIR && jevify fill …` changes it.
- Hidden files are listed; `.git/` never is. Excerpts are never read for a path with a dot
  component, nor for `.env*`, `*.pem`, `*.key`, `id_*`, `*credentials*`, `*secret*`, `.netrc`,
  `.npmrc`, `.envrc`. The status line says `excerpts withheld: N`. Symlinks are not followed.
  The policy is one function in `src/records.rs`, delivered by bead 1.1, and it applies to
  `--files` on every verb; the `file` kind calls it. Phase 1 proves it: a captured request for
  `filter --files` holds no content of a `.npmrc`, and the status line says
  `excerpts withheld: 1` (`tests/filter.rs`).

### 2.3 Records and the scorer

`records.rs` reads bytes through `input::read_stdin_bytes()`. `input::read_stdin` decodes
lossily and trims (`src/input.rs:43-49,69`); it stays as it is for `why` and `is`. `why` is
outside the record model (§1 rule 1): it takes no split option and keeps its output format.

- **The handle is an `OsString`**: `Record { handle: OsString, evidence: String,
  raw: Range<usize> }`. It keeps every byte, so `fill` substitutes a path that is not UTF-8
  unchanged. Inline tests in `records.rs` and a contract test in `tests/fill.rs` use a
  non-UTF-8 handle and a non-UTF-8 path.
- **Splitting**, exactly one of: lines (default), `-0`, `--para`. Two together is exit 2.
- **`--files`** is a boolean on `pick`, `filter` and `label`: the record is a path and the
  evidence is its first lines, so `fd -0 | jevify filter -0 --files '…'` works. The order is
  fixed: the withholding function of 2.2 runs first; a relative path is then resolved against
  the working directory, because `sort::excerpt` needs an absolute normalized path
  (`src/cmd/sort.rs:46-47`); every excerpt read runs inside `spawn_blocking`, because
  `excerpt` reads files and may start `pdftotext`. `pick --files` keeps its two stages: path
  names first, excerpts for the finalists.
- **A machine format and a record that is not UTF-8.** The envelope carries the record's
  `text` as lossy UTF-8, `lossy: true`, and its 1-based `ordinal`, so the caller recovers the
  bytes from its own input or from the saved input. No base64. `tests/pick.rs`,
  `tests/filter.rs` and `tests/label.rs` each hold the case.
- **JSON is `jq`'s job.** `jq -c '.[]'` turns an array into lines, a verb selects lines, and
  `jq -r .number` reads the field. Output verbs have no JSON mode.
- **`fill` alone reads a handle out of a record**, because it substitutes one: `--field N`, the
  N-th whitespace-separated field, as awk; `--key NAME`, a top-level key of a JSON value, where
  the input is JSON lines or one array. The evidence is the whole record.

Blank records are dropped. Records come out as the bytes they came in, in input order: CRLF,
colour codes and bytes that are not UTF-8 survive. Evidence alone is decoded, ANSI-stripped,
clipped and redacted. Byte-identical records are judged once. Input above 64 MiB is exit 6
`input_too_large`, as today.

The scorer (`filter`, `label`):

- **One request judges up to 1,000 records, each alone.** classifier.dev takes `inputs` (up to
  1,000 texts) with `dimensions` in one `POST /v1/classify`, at most 1,000 decisions per
  request, and answers each input on its own. `Client::ask_each` sends that. jevify sends one
  input and 20 dimensions per request today (`src/jev/classifier.rs:119-150`), which is the
  right shape for `pick`, `why`, `is` and `fill`, and the wrong one for per-record verbs:
  10,000 records are 10 requests, not 500, and no record's answer leans on its neighbours.
  On TypeSafe, `ask_each` sends 20 records in one state, as `add.rs:10` does; each question
  names its record by id and says to judge it alone. **Independence of records is a property
  of the classifier backend only.** `filter A | filter B = filter B | filter A` is a contract
  test on FakeJev, not a claim about the model.
- **`ask_each` on the wire.** `Client::ask_each(records: &[String], questions: &Questions)
  -> impl Stream<Item = Result<(usize, Vec<Response>), JevifyError>>` yields one item per batch:
  the batch index and one `Response` per record, in record order.
  - classifier.dev: the body is `{"items": [r1, …, rN], "dimensions": {…}}`, the body
    `classifier::request_body` builds today with one item (`src/jev/classifier.rs:161`).
    `request_body` takes a slice of items. `N × questions.len() ≤ 1,000`; each item keeps the
    32,000-character limit (`MAX_INPUT_CHARS`), and a longer record is clipped as evidence is
    today. `classifier::parse` reads the first result only (`:200-204`); it becomes
    `parse_each`, which maps every element of `results` through the same per-dimension code and
    fails with a protocol error when `results.len() != N`.
  - A Noul goes out as two semantic labels, as today (`noul_labels`, `:45-55`): for `filter`
    the labels are "the statement holds" and "the statement does not hold", and the statement
    goes into `instructions` together with "judge this one record".
  - TypeSafe: no batch form. `ask_each` builds one state with 20 records and one question per
    record, the form of `add.rs:10`; each question names its record by id and says to judge it
    alone. It unpacks the answers into one `Response` per record.
  - Redaction runs per record (`input::redact`), before the cache key is made. The cache key is
    per batch, as it is per request today (`src/jev/client.rs:260-266`), so a second run over
    the same input is answered from the disk.
  - Batches go through `Client::post` and its semaphore and retry policy unchanged.
  - `ask_classifier` sends its 20-question chunks one after the other today
    (`src/jev/client.rs:324-329`). It sends them at the same time (`try_join_all`), so `is`
    with many statements and `fill` with many context markers stay one round.
- **Flow.** `filter` and `label` hold a `BTreeMap<usize, Vec<Response>>` of finished batches and
  a cursor. When the batch at the cursor arrives, its kept records are written and the cursor
  moves; later batches wait in the map. Memory is bounded by the input, which is in memory
  already. A write error of kind `BrokenPipe` drops the stream, which cancels the queued
  requests, and the verb ends with exit 0. `filter 'x' | head -5` ends early. With a machine
  format nothing flows and the envelope comes at the end.
  - `run_cli` writes `Outcome.human` after dispatch (`src/lib.rs:151-158`), which is too late
    for a flow. On the human path `filter` and `label` write their records straight to a
    locked stdout, handle `BrokenPipe` as exit 0 there, and return an empty `human`.
- **The model.** Every `Response` carries `model` (`src/jev/mod.rs:89`), and classifier.dev
  names it per dimension (`src/jev/classifier.rs:217-219`). `meta.model` stays a string
  (`src/output.rs:75`, `src/config.rs:195`; `tests/classifier.rs:63` and `tests/live.rs:77`
  assert a string): distinct contributing models are joined with ", " in first-seen order,
  and `Stats.model` holds the joined string, so `src/output.rs` and `src/config.rs` do not
  change. Every model that contributed is kept through `classifier::parse_each`, the chunk
  merge in `ask_classifier` and the disk cache; today `src/jev/classifier.rs:217-218` and the
  chunk loop overwrite it, and `src/jev/client.rs:278` keeps the last one. A part that does
  not start with `jev` is named in the status line:
  `answered by ibm-granite/granite-4.0-h-micro, not Jev`. The model guard of `fill` requires
  every part to start with `jev`. Test: a non-Jev dimension followed by a Jev dimension makes
  `fill` refuse (`tests/client.rs` for the joined string, `tests/fill.rs` for the refusal).
- **The pace is the backend's.** Keyless: 3,000 decisions a minute and 20,000 a day per IP.
  The verb prints `jevify filter: 10074 records, 3120 distinct, 4 requests` on stderr before
  the first request.
- **429.** The daily limit is HTTP 429 with a JSON body whose `code` is `rate_limit_day`
  (classifier.dev OpenAPI). It is never retried: exit 4, message "daily quota of the free
  backend reached"; the printed records are a correct prefix and the last line says
  `answered 3000 of 3120`. `rate_limit_minute` and every other 429 are retried after
  `Retry-After` (`src/jev/client.rs:418-422`). For `ask_each` the cap on one wait is 60 s (it
  is 10 s today, `retry_after`, `src/jev/client.rs:442-451`), because a minute limit needs a
  minute. The FakeJev quota responder returns exactly that status and body.
- **The ceiling** is 20,000 distinct records, a day of the free backend: exit 6 `too_many`, and
  the message says to narrow with `grep` or `head`. There is no flag to raise it.
- **`filter` flags:** `-v` keeps the records where the statement is false, `-c` prints the
  count, `--strict` drops the unsure. Unsure records are kept in both directions. Line numbers
  and context come from `grep` over the saved input:
  `grep -n -C2 -F -f <(jevify filter 'x' < build.log) build.log`.

### 2.4 The saved input

`why` and `filter` write the full input, as the bytes they read, to
`<save dir>/outputs/<blake3-16>.log` before the first request, and print `full output: PATH` as
their last stderr line. Only these two save. stdin `pick` saves nothing: it returns one record
of a list the caller can list again. `label` saves nothing: every record comes out.

- The name is the content hash: no slot, no index file, no lock. The write goes through a
  temporary name and a rename, as `src/jev/cache.rs:42-53` does. Directory 0700, file 0600.
- jevify never deletes these files. `capabilities` and `PRIVACY.md` name the directory.
- `<save dir>` is `JEVIFY_CACHE_DIR` (`src/config.rs:99`) or the platform cache directory. It
  does not depend on the answer cache. `--no-save` skips the save.
- The file holds the raw input, secrets included. `PRIVACY.md` and the skill say so.
- A failed or skipped save prints `full output: not saved (<reason>)` and sets
  `data.complete=false`.

## 3. Phases

One shared checkout, `main` only. Every bead reserves its exact write set through Agent Mail.
Each phase has the same shape: a serial **skeleton** bead that owns the shared files
(`Cargo.toml`, `src/cli.rs`, `src/lib.rs`, `src/cmd/mod.rs`, `src/exit.rs`, `src/input.rs`,
`src/output.rs`) and leaves the tree green; **parallel** beads that each own files nobody else
writes; a **contract** bead that owns `src/cmd/agent.rs`, the documents, the skill and the
manifests, then the version bump and the release through `bash scripts/release.sh`.

- **A contract bead waits for the behaviour it documents**, so it never closes before the
  status lines it quotes exist. The "After" columns below say which beads.
- **The tracker.** Workers change bead state with `br` only and never stage `.beads/`. The
  lead exports (`br sync --flush-only`) and commits `.beads/` between waves.
- **A release bead runs `ubs` on every file it changes**, `Cargo.lock` included. It does not
  require the epic closed; the epic closes when its children close. The evaluation bead
  hunch-lpp is no child of the epic: it depends on the 0.7.0 release bead only.

### Phase 0 — base URL (hunch-ed1)

`JEVIFY_BASE_URL` must be `https`. The TypeSafe backend accepts only `api.typesafe.ai`, the
classifier backend only `classifier.dev`; `127.0.0.1` and `localhost` accept any scheme, for
wiremock. A URL with userinfo is refused. Redirects are not followed, and that covers the
`health` client, which `src/cmd/agent.rs:127` builds on its own with reqwest's default policy.

Host rules: hosts are compared case-insensitively (`API.TypeSafe.AI` is accepted); a trailing
dot is refused (`api.typesafe.ai.`); an explicit port other than 443 is refused on the two
known hosts (`:443` is accepted, `:8443` is not). `https://api.typesafe.ai.evil.example` and a
value that is not a URL are refused. An empty value is the default URL, as for every variable
(`env`, `src/config.rs:73-78`). `http://127.0.0.1:PORT` is accepted.

Writes: `src/config.rs`, `src/jev/client.rs`, `src/cmd/agent.rs`, `tests/client.rs`,
`tests/agent.rs`, `docs/guide/configuration.md`, `PRIVACY.md`, `CHANGELOG.md`.
Done when `tests/client.rs` holds: plain http refused, unknown host refused, a TypeSafe key
with `classifier.dev` refused, userinfo refused, a 302 not followed, localhost accepted; and
`tests/agent.rs` holds: a redirected `health` request never reaches its target.

### Phase 1 — the language: cutover, records, `filter`, `is` (0.5.0)

Goal: one grammar, one record model, and the output side an agent uses most.

| Bead | Writes | After |
|:---|:---|:---|
| 1.0 skeleton — new: language skeleton and grammar cutover | the shared files; `src/config.rs` (save dir); stubs `src/records.rs`, `src/save.rs`, `src/cmd/filter.rs`; `src/cmd/why.rs` + `tests/why.rs` (delete `capture`, the `cmd` argument); the `Outcome` change (`human: Vec<u8>`, `exec: None`) in every `src/cmd/*.rs`; the final `route` signature in `src/cmd/run.rs`; `tests/run_args.rs`, `tests/run_route.rs`, `tests/cli_basics.rs`, `tests/pick.rs` and `tests/agent.rs` where the removed flags force it | Phase 0 |
| 1.1 — new: byte records | `src/records.rs` (with the withholding function) | 1.0 |
| 1.2 — new: `ask_each` | `src/jev/client.rs`, `src/jev/classifier.rs`, `src/jev/mod.rs`, `tests/client.rs`, `tests/classifier.rs`, `tests/common/mod.rs` (the batch responder, the probability vectors, the quota responder) | 1.0 |
| 1.3 — new: saved input | `src/save.rs` | 1.0 |
| 1.4 — new: route is print-only | `src/cmd/run.rs`, `src/manpage.rs`, `tests/run_route.rs`, `tests/run_args.rs`, `tests/live.rs` (`live_run_routes_tar` moves to `route` with its assertions); removes `mod args;` from `src/lib.rs` | 1.0 |
| 1.5 — hunch-bx6 (filter and the scorer) | `src/cmd/filter.rs`, `tests/filter.rs`, `tests/live.rs` (one added test) | 1.1, 1.2, 1.3, 1.4 (`tests/live.rs`) |
| 1.6 — new: `is` with several statements and `--context` | `src/cmd/is.rs`, `tests/is.rs` | 1.0 |
| 1.7 — new: save in `why`, records in `pick` | `src/cmd/why.rs`, `tests/why.rs`, `src/cmd/pick.rs`, `tests/pick.rs` | 1.1, 1.3 |
| 1.8 contract, documents — hunch-7zg.8 | section 6 (README, `docs/`, `src/cmd/agent.rs`, `AGENTS.md`, `PRIVACY.md`, `CHANGELOG.md`) | 1.4, 1.5, 1.6, 1.7 |
| 1.8 contract, skill — hunch-sb9 | the skill and the two manifests | 1.4, 1.5, 1.6, 1.7 |
| 1.9 — new: scripts and demo files use the new grammar | `demo.tape`, `demo-record.sh`, `scripts/eval_run.py`, `scripts/eval_why.py`, `benchmarks/bench.sh`, `docs/img/run.svg` | 1.8 documents |
| release 0.5.0 (lead) | the version, `Cargo.lock`, the `CHANGELOG.md` heading | all of the above |

**The cutover of `run` happens in 1.0.** Bead 1.0 removes `--yes`, `--exec`, `--dry-run` and
`--no-args` from the parser, sets the final signature `cmd::run::run(ctx, intent, machine)`
and removes `RunFlags` (`src/lib.rs:257-275`). The five execution tests of `tests/run_args.rs`
and the two tests of `tests/run_route.rs` use those flags, so 1.0 rewrites these seven tests
to exit-2 and print-only assertions. This is the owner-approved removal of `run`, not a
weakened test. Bead 1.4 then deletes the execution code behind the signature. Bead 1.0 also
updates `QUICK_START` (`src/lib.rs:37-51`) and its assertion
(`tests/cli_basics.rs:116`, `bare_jevify_prints_the_quick_start_card_as_a_usage_error`); 2.0
and 4.0 add their line to it. Bead 1.0 adds the parser value `agents` to `Init` and its
dispatch arm; bead 1.8 writes the block.

**The cutover of `pick --files` spans 1.0 and 1.7.** Bead 1.0 makes `--files` a boolean
(`src/cli.rs:66-67`), makes `src/cmd/pick.rs` take the paths from its stdin lines where it
walks `DIR` today (`:27-34`, `list_files` `:152`), and moves the `--files` tests of
`tests/pick.rs` (`:137-`, `:208`, `:230`) to the stdin form with their assertions, so the tree
stays green. Bead 1.7 puts `pick` on the byte records, adds `-0 --files`, the withholding
function and the relative-path rule of 2.3, and completes those tests in place. `why` gets no
`-0`, `--para` or `--files` in the parser.

`is 'a' 'b' 'c'`: one Noul per statement in one request; exit 1 when any is no, else 3 when
any is unsure, else 0. One statement prints nothing on stdout, as today; two or more print one
verdict line per statement, in the order given: `VERDICT<TAB>STATEMENT` with `yes`, `no` or
`unsure`, so `cut -f1` reads the verdicts. Probabilities stay on stderr and in the envelope.

Done when, in this repository, on the default backend:
```
cargo test 2>&1 | jevify filter 'reports a failed assertion'      # records, then: kept K of N, U unsure, full output: PATH
seq 1 10074 | jevify filter 'x' | head -3                         # prints early, ends early, exit 0
gh pr list --json number,title | jq -c '.[]' | jevify filter 'touches Windows paths' | jq -r .number
fd -0 -e rs . src | jevify filter -0 --files 'spawns a child process'
jevify is 'a Rust crate manifest' 'names a binary target' --context Cargo.toml; echo $?
jevify route 'inspect Mach-O metadata'                            # prints a tool, starts nothing
jevify run 'x'; jevify why -- true                                # both exit 2 with the new form
```
and, as contract tests: 2,500 records on the classifier backend send three requests; 20,001
distinct records send none; coloured `rg` output comes out byte-identical; a record that is
not UTF-8 is `text` + `lossy: true` + `ordinal` in the envelope; an unwritable save directory
gives `not saved`; a 429 with `code: rate_limit_day` is not retried and leaves a correct
prefix, and a `rate_limit_minute` 429 is retried after `Retry-After`; an answer from a model
that is not Jev is named in the status line and joined into `meta.model`; `filter --files`
sends no content of a `.npmrc` and says `excerpts withheld: 1`; `filter -v` inverts and
`is 'x' -v` is exit 2; `is` with one statement prints nothing and with two prints two lines;
`git ls-files | jevify pick --files 'q'` prints a path and `pick --files DIR` is exit 2.

### Phase 2 — `fill` (0.6.0)

Goal: one call replaces "list, read, choose, act", for any list an agent can pipe, for
branches, and for an option that depends on text it has not read.

| Bead | Writes | After |
|:---|:---|:---|
| 2.0 skeleton | the shared files (`Fill`, `Pick --from`, the error kinds, the `fill` line of `QUICK_START`); `run_cli` acts on `Outcome.exec` (no new field); `tests/bin/argv.sh`; stubs `src/marker.rs`, `src/source.rs`, `src/cmd/fill.rs` | release 0.5.0 |
| 2.1 — new: marker lexer | `src/marker.rs` | 2.0 |
| 2.2 — hunch-q8p (`-`, `branch`, the lister runner) | `src/source.rs` | 2.0 |
| 2.2 — hunch-zxz (the pool rule for every caller of `rank`) | `src/tournament.rs`, `tests/tournament.rs`; call sites and capacity tests in `src/cmd/why.rs`, `src/cmd/pick.rs`, `src/cmd/run.rs`, `tests/pick.rs`, `tests/why.rs` | 2.0 |
| 2.3 — new: `pick --from` | `src/cmd/pick.rs`, `tests/pick.rs` | 2.2 (both) |
| 2.4 — hunch-bkb (fill core) | `src/cmd/fill.rs`, `tests/fill.rs`, `tests/live.rs` | 2.1, 2.2 (both) |
| 2.5 contract | section 6 | 2.1, 2.2 (both), 2.3, 2.4 |
| release 0.6.0 (lead) | the version, `Cargo.lock`, the `CHANGELOG.md` heading | all of the above |

Done when, in this repository, on the default backend:
```
jevify fill --dry-run -- git switch '@{branch:the release automation work}'
git log --format='%H %s' -3000 | jevify fill --field 1 -- git show '@{-:made folder moves atomic}'   # candidates 3000, windows 31
gh pr list --json number,title | jevify fill --dry-run --key number -- gh pr view '@{-:the Windows path fix}'
jevify fill -- git switch '@{branch:a branch that does not exist}'; echo $?      # 3, nothing ran
printf 'x\n' | jevify fill -- true '@{-:x}' '@{flag:--draft:y}'; echo $?          # 2, stdin has two roles
jevify fill -q -- false '@{-:x}' <<< x; echo $?                                  # 1, the command's own code
jevify pick --from branch 'the release automation work'
```
and, as contract tests: the `gh issue create` example of the vision sends one POST for its
three markers; the sh helper receives the exact argv, a non-UTF-8 handle included; an unsure
`flag` and an answer from another model run nothing (a sentinel file proves it), and so does
an answer whose `meta.model` has one part that is not Jev; `F + 1` lines are `too_many` with
no request; 250 lines resolve in two rounds and the finals hold three finalists of every
window; on the classifier backend 4,000 items give `n = 2` in one finals request and 9,802
items are `too_many` with no request, for `pick` and for `pick --from`; `why` with
`MAX_KEEP` lines resolves with `n = 2`; a `one` with `W` options resolves and with `W + 1` is
exit 2; several failed markers report the first in argv order and list all in
`data.markers[]`; `cargo metadata` shows one bin target, `jevify`.

### Phase 3 — kinds

The recipe engine, the shipped recipes, the user's `kinds.jsonl`; `commit`, `file`, `dir`,
`tool`. All in `src/source.rs` and `src/kinds.jsonl`, serial, then `tests/fill.rs` and
`tests/pick.rs`, then the contract bead with `docs/guide/kinds.md`. The recipe bead adds
`JEVIFY_CONFIG_DIR` to `src/config.rs` and removes it in the sanitized test command of
`tests/common/mod.rs`; the contract bead documents it in `docs/guide/configuration.md`. The
`file` kind calls the withholding function of `src/records.rs` and does not define a second
one.

Done when `'@{commit:made folder moves atomic}'` prints a full OID under `--dry-run`; a log
above the limit says `candidates N of M, newest first` with `M` from `git rev-list --count`;
a lister that sleeps, a lister whose child keeps the pipe open and a lister that prints more
than 64 MiB each give `lister_failed` within the deadline plus 200 ms; a bad line in the
user's `kinds.jsonl` is `recipe_invalid` with its line number for any user kind, and is never
read for `branch` or `pr`;
`'src/cmd/@{file:stages hunks}'` becomes `src/cmd/add.rs`; a tracked `.npmrc` reports
`excerpts withheld: 1` and its content is absent from the captured request; a line appended to
a temporary `kinds.jsonl` makes `'@{widget:…}'` resolve, the same line named `branch` is
`recipe_invalid`, and a `kinds.jsonl` in the working directory is never read; a fake `gh` that
prints "not logged in" gives `lister_failed` with that text.

### Phase 4 — `label` (hunch-qxn)

`label a,b,c`: one Choice per record, the labels plus an internal `NONE`, through `ask_each`.
Output is `LABEL<TAB>RECORD`; an unsure record has the label `?`. Needs Phase 1 only for its
logic; it runs after release 0.6.0 because it shares the skeleton files. At most `W` labels
(99 on classifier.dev, 200 on TypeSafe), because `NONE` takes one slot: more is exit 2 with
the count, checked in `label` where the backend is known.

The way back: for line records, `cut -f2-` of the output equals the input without its blank
lines. With `-0` and `--para` the label prefixes the first line of the record, and the claim
there is "the record follows the tab unchanged". `label` saves nothing.

Done when `gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c` prints
a histogram; `cut -f2-` of the output is the input without its blank lines (a test holds a
blank line); under `--para` a paragraph that holds a tab follows the first tab unchanged; `W`
labels resolve and `W + 1` are exit 2 on each backend; a record that is not UTF-8 is
`text` + `lossy: true` + `ordinal` in the envelope.

### After the language is stable

Evaluation follows the language, on a surface that no longer moves (hunch-lpp, after release
0.7.0; each item is an acceptance criterion of that bead): a local invocation log; a held-out
set per verb and kind, labelled by someone other than the author; recall of `filter` at the
unsure boundary; how often the right candidate misses the finalists of its window, for
`n` = 3, 2 and 1; Jev as a judge over the agent transcripts.

### Later

Each item was cut on purpose and names what brings it back.

- `filter -e`, `-n`, `-A/-B/-C`: the `grep -F -f` form is shown to be too clumsy.
- `fill -C DIR`: a caller cannot `cd`.
- `--evidence` and a recipe field for it: a lister cannot choose its own fields.
- An identity re-check before `exec`: a wrong act is traced to a candidate that changed between
  the listing and the run. The command's own error covers a candidate that is gone.
- `test`, `script`, `complete` as kinds: the pipe
  `cargo test -- --list | sed 's/: test$//' | jevify fill -- cargo test '@{-:…}'` is shown to be
  too clumsy. These listers run project code, so they return with a consent flag.
- A recipe field for tier-two evidence: a recipe kind abstains as `ambiguous` too often.
- `label --ordered`, a Score wire type, `--report FILE`, a third round above `F`, a setting for
  the ratio, pruning of saved inputs (only with the owner's permission).

## 4. clap and parsing

- `fill`: `#[arg(last = true)] cmd: Vec<OsString>`; a command without `--` is exit 2 with the
  corrected example. `trailing_var_arg` stays off.
- Output verbs do not take `allow_hyphen_values`. Text that starts with `-` goes after `--`.
- `pick -n N` and `why -n N` are "top N". `why -C N` is context lines. `why` has no `-0`,
  `--para` or `--files`.
- `is` takes one or more statements (`Cmd::Is { condition: String }` today, `src/cli.rs:110`).
- `-v` belongs to `filter` alone. The global `--verbose` has no short flag
  (`#[arg(short, long, global = true)]` today, `src/cli.rs:36`).
- `--files` is a boolean on `pick`, `filter` and `label` (`files: Option<PathBuf>` on `pick`
  today, `src/cli.rs:66-67`). `pick --index` stays; `--index` with `--files` stays exit 2.
- `Init { shell }` takes `zsh`, `bash` or `agents` (`Shell::{Zsh,Bash}` today,
  `src/cli.rs:154-161`); the enum is renamed if that reads better. Bead 1.0 adds the value and
  the dispatch arm.
- `Route { intent }` has no flag of its own. `cmd::run::run(ctx, intent, machine)` is the final
  signature, set in bead 1.0.
- A `one` marker and `label` take at most `W` options. The parser has no backend, so `fill`
  and `label` check it after `Config::load`: exit 2 with the count.

## 5. Tests

- `marker.rs` table: every literal (`@{u}`, `@{-1}`, `HEAD@{2}`, `stash@{0}`, `@{1 day ago}`,
  `@types/node`, `user@host:path`, PowerShell `@{k='v'}`), prefix,
  suffix, flag value, two markers in one argument, `\}`, quotes stripped once, unterminated,
  unknown kind with suggestion, empty body, a marker in `argv[0]`, a non-UTF-8 literal,
  `one` and `flag`, substituted text that holds `@{`, stdin with two roles. Python
  `{user}@{host:>8}` is in the error rows: unknown kind `host`, exit 2, and the message shows
  `'{user}@@{host:>8}'`; `{user}@@{host:>8}` is in the literal rows.
- `FakeJev` gives the winner 0.9 and spreads 0.1 (`tests/common/mod.rs:26-31`), so no tie and
  no `ambiguous` path can be written today. Bead 1.2 adds a probability vector per question,
  the batch responder, a named model per dimension, and the quota responder (HTTP 429 with
  a JSON body whose `code` is `rate_limit_day`, in the error shape of the classifier.dev
  OpenAPI, and the `rate_limit_minute` form with `Retry-After`);
  `FakeClassifier` is built from it.
- `tests/tournament.rs`: the pool rule with `n` = 3, 2 and 1; 4,000 and 9,802 items on the
  classifier backend; the planted answer in every position of every window.
- `tests/fill.rs`: dry-run output equals the argv a run receives (`sh tests/bin/argv.sh`
  prints its arguments NUL-separated); a non-UTF-8 handle; `W` and `W + 1` options in a `one`;
  several failed markers; abstain prints nothing and runs nothing; 0, 1 and 2 candidates; a tie;
  `NONE` close to the best; same evidence with different handles; a handle with a newline or a
  leading `-`; `W`, `W + 1`, `F`, `F + 1` candidates on each backend; two close rivals in one
  window both reach the finals and end as `ambiguous`; the right candidate in the last window
  wins; a branch with a remote twin resolves; stdin ownership (the command sees EOF with
  `@{-:…}`, inherits otherwise); the command's exit code comes through; `-q`; a machine format
  without `--dry-run` is exit 2; `one` + `flag` in one request; a `flag` no removes the
  element; a `flag` unsure and a model that is not Jev run nothing.
- Output verbs: split modes and their exit-2 combinations; CRLF, colour, NUL and non-UTF-8
  records byte-identical; `ask_each` batches of 1,000 on the classifier backend and 20 on
  TypeSafe; judge-once; flow (the first batch is on stdout before the last is answered); a
  closed stdout; the ceiling; the prefix after a `rate_limit_day` 429, and the retry after a
  `rate_limit_minute` 429; `-v` (inversion; `--verbose` is separate), `-c`, `--strict`;
  `filter A | filter B` equals `filter B | filter A` on FakeJev (a contract test, not a claim
  about the model); `label … | cut -f2-` equals the input without its blank lines, and a
  paragraph with a tab under `--para`; `is` with one statement prints nothing and with two
  prints two lines; `is --context FILE` equals `is < FILE`; the saved path, two saves, the
  failed save; `--files` with a relative path, with a `.npmrc` (withheld, counted, absent from
  the request); a non-UTF-8 record under a machine format (`text`, `lossy`, `ordinal`);
  `pick --files` from stdin. `why` keeps its format tests; it has no split-mode test.
- Recipes: every shipped line parses; a bad line and a shadowed kind are `recipe_invalid`; a
  bad line is not read when the kind is coded or shipped; the working directory is never
  searched. Inline tests of `source.rs` pass an injected `PATH` and deadline through
  `Command::env`; the tests that need the process environment (fake `git` and `gh`, user
  recipes through `JEVIFY_CONFIG_DIR`, `tool`) are in `tests/fill.rs` and `tests/pick.rs`
  through `assert_cmd`'s `.env`. No `set_var` in `src/`. The runner: a sleeping lister, a
  child that keeps the pipe open, an oversized output.
- Live tests (ignored by default) for one `branch`, one `one` and one `filter` batch per
  backend. Without a key they are reported as not run.

## 6. The README, the skill and the `AGENTS.md` block

Three documents teach one language to three readers, in one order: the two sides, the verbs,
the marker, the exit codes. Bead 1.8 rewrites all three; each later contract bead adds what its
phase delivers. None names a verb, a flag or a kind the installed release lacks. For 0.5.0 the
order is "the two sides (output side only), the verbs, the exit codes"; the marker joins in
2.5.

Bead 1.8 (documents) also owns the repository's `AGENTS.md`: the sentences on the `run`
argument pass (`:14`), on `why -- <cmd>` (`:24`) and on exit 7 in the contract list, and the
text of `docs/img/`. Its acceptance greps are anchored, so that prose such as "a paid run"
does not match: `jevify run`, `why -- `, `jevify fill`, `jevify label`, `@\{[a-z-]+:`. Bead 1.9
gives the scripts and demo files the grammar (`demo.tape`, `demo-record.sh`,
`scripts/eval_run.py`, `scripts/eval_why.py`, `benchmarks/bench.sh`, `docs/img/run.svg`; they
call `jevify run`, `why -- cargo build` and `jevify -v` today), and hunch-4ow records
`demo.gif` after it. hunch-60l (the README voice rewrite) waits for the last contract bead,
so two beads never hold `README.md` together.

**README, for a person.** Today it has a section per verb in build order and `jevify run` on
its first screen. The rewrite:

| Section | Content |
|:---|:---|
| One sentence, one figure | "jevify gives command-line tools an understanding of meaning", and the two-sides figure |
| Thirty seconds | install, no key, three commands: a `filter` on a log, an `is` in an `&&` chain, a `fill --dry-run` on a branch |
| The output side | the verb table with twins; six lines of composition with `jq`, `cut`, `grep`, `fd`, `head`; the way back |
| The input side | `fill`, the marker, the three families, `--dry-run`, all-or-nothing |
| Kinds | the table; a recipe in one line; where `kinds.jsonl` lives |
| Exit codes | the table, and "write the condition so that yes means act" |
| For agents | the skill, `jevify init agents`, the permission rule |
| What it is not; `add`, `sort`, `route`; privacy, limits, license | short |

The "Numbers" section moves to `docs/guide/how-it-works.md` until the evaluation refills it.

**Skill, for a Claude agent** (`plugins/jevify/skills/jevify/SKILL.md`). It teaches
`jevify run`, `why -- CMD`, a shell loop over `is` and `"$(… | jevify pick …)"` today; the
language replaces all four.

- Triggers: about to list branches, commits, PRs or runs only to choose one → `fill`; a tool
  can list it → pipe into `fill … '@{-:…}'`; want the value, not the run → `pick --from`; an
  option depends on text you have not read → `@{one:…}` / `@{flag:…}`; a failed build over
  about 50 lines → `2>&1 | jevify why`; many records, one question → `filter`; many files, one
  question → `fd -0 | jevify filter -0 --files`; every record needs a bucket → `label`; the
  next step depends on a fact → `is … &&`; waiting for a state → `until … | jevify is '…'`.
- Habits: one jevify process per question, never a loop that starts one per record; cheap
  tools first; yes means act; never `"$(jevify pick …)"` as an argument, because an abstention
  becomes an empty argument; the whole marker argument in single quotes, an apostrophe is
  `'\''`; `--dry-run` to look, never `eval`; a marker does not survive a second shell (`ssh`,
  `make`, `xargs`); under `git bisect run`, map exit 3 to 125.
- Recovery: exit 2 → copy the corrected argument; `too_many` → a prefix, `grep`, `head`, or a
  narrower pipe; `ambiguous` → read the two handles, write one; `no_match` → read
  `candidates N of M`; `unsure_flag` → write the flag or drop the marker; `lister_failed` → run
  the named lister yourself; exit 4 → the quota or the model, read the line.
- Permissions: allow the output verbs and `jevify fill --dry-run` freely. Allow `fill` per
  command prefix (`jevify fill -- git switch:*`), exactly as the command itself is allowed.

**`AGENTS.md` block, for every other agent.** `jevify init agents` prints at most 25 lines:
the triggers, the verbs, one `fill` example with the quoting habit, the exit codes, and
`jevify capabilities --json` as the source of truth. It is built from the `capabilities` table.

## 7. Privacy and time

The description, the context of `one` and `flag`, and the records under judgment leave the
machine, masked on a best-effort basis by `input::redact`. Paths of hidden files leave as names;
their content does not. Two stores exist: the answer cache (redacted, seven days, `--no-cache`)
and the saved inputs (raw, never pruned, `--no-save`).

Nothing rations requests. What bounds a verb is time: two rounds, the client semaphore
(`JEVIFY_CONCURRENCY`), one lister deadline, the backend's pace, the ceiling, 64 MiB.

## 8. Risks

1. **A wrong pick runs.** Guards: `NONE`, the `any` Noul, the ratio, finals that hold every
   window's finalists, tier two, one candidate per branch, an unsure `flag` abstains, the model
   guard, all-or-nothing, the evidence line. A window can hold more than three candidates that
   beat the right one; the finals' `NONE` and `any` are the guard then. `fill` never goes
   below three finalists per window; `pick` and `why`, which run nothing, go to two and one.
2. **Agents misquote the marker.** Every damaged marker the lexer can see is exit 2 with the
   corrected spelling. `$x` expanded inside double quotes is invisible; the single-quote habit
   is the guard.
3. **The command's exit code can be 2 to 6.** The `exec` line on stderr says whose code it is.
4. **The free backend answers with another model, or its quota ends.** The model guard over
   every part of `meta.model`, the model in every status line, `rate_limit_day` is exit 4 with
   no retry, a correct prefix.
5. **Saved inputs hold secrets and grow.** 0600 in 0700, `PRIVACY.md`, `--no-save`.
6. **A user recipe runs a command.** Only from the user's configuration directory; it cannot
   shadow a shipped kind; `capabilities` prints its argv.
7. **A lister hangs, or leaves a child that holds the pipe.** The runner of 2.2: a `try_wait`
   poll, `kill` on the deadline, 200 ms for the readers, then `lister_failed`.
8. **`--files` reads a secret.** One withholding function in `src/records.rs`, called before
   any excerpt read, on every verb and in the `file` kind.
9. **Records share a state on TypeSafe.** Each question names its record and says to judge it
   alone; independence is claimed for the classifier backend only.

## 9. Owner decisions

1. `git mv src/cmd/run.rs src/cmd/route.rs`, `tests/run_route.rs` → `tests/route.rs`, and the
   removal of `src/args.rs` and `tests/run_args.rs`. Without permission the files keep their
   names and `src/args.rs` stays on disk outside the module tree.
2. `fill` becomes the command through `exec`. The command's exit code comes through, and exit 7
   leaves `fill`. The alternative, a child with signal forwarding, keeps one exit contract and
   costs the signal code, a dependency feature and four test families.
3. An unsure `flag` abstains.
4. `fill` resolves through windows up to `F` in two rounds; the pool is never cut. One pool rule
   serves every caller of `tournament::rank`: `n` = 3, 2 or 1 finalists per window by rank,
   capacity `W × W`. `fill` uses `n = 3` only; `pick`, `pick --from` and `why` use the full
   rule; `why` keeps `MAX_KEEP = 4000`.
5. Per-record verbs use the batch request of classifier.dev; `fill` refuses an answer that is
   not from Jev in every part. `meta.model` stays a string, joined with ", ". Independence of
   records is a property of the classifier backend only.
6. Cut on purpose, listed under Later: `-C DIR`, `--evidence`, the identity re-check, the
   `test`, `script` and `complete` kinds with their consent flag, `filter -e/-n/-A/-B/-C`,
   `label --ordered`, `--max-records`.
7. Saved inputs are content-addressed and never pruned by jevify. Only `why` and `filter` save.
8. `filter -v` is inversion; the global short `-v` leaves and `--verbose` stays.
9. `--files` is a boolean on `pick`, `filter` and `label`; `pick --files DIR` leaves with no
   shim; `pick --index` stays.
10. `why` is exempt from the byte-for-byte rule and takes no split option.
11. `is` with one statement prints nothing; with several, one verdict line each.
12. The removal of `run`'s flags in bead 1.0 rewrites seven tests of `tests/run_args.rs` and
    `tests/run_route.rs` to exit-2 and print-only assertions. It is the approved removal of
    `run`, not a weakened test.
13. `rate_limit_day` is never retried; one wait of `ask_each` is capped at 60 s.
14. `{user}@{host:>8}` is an unknown kind, exit 2; the literal is spelled `@@{`.
15. A handle is an `OsString`. A record that is not UTF-8 is `text`, `lossy: true` and
    `ordinal` under a machine format; no base64.
16. `one` and `label` take at most `W` options.
17. The test helper is `tests/bin/argv.sh`; the crate keeps one bin target.
18. The absolute path in `source_repo_path` of the tracker export (hunch-nay) does not block
    this work.

## 10. Beads

| Phase | Existing beads | New beads |
|:---|:---|:---|
| 0 | hunch-ed1 | — |
| 1 | hunch-bx6 (1.5), hunch-sb9 (1.8) | skeleton and cutover (1.0); byte records (1.1); `ask_each` (1.2); saved input (1.3); route is print-only (1.4); `is` statements and `--context` (1.6); save in `why` and records in `pick` (1.7); 0.5.0 documents (1.8); scripts and demo files use the new grammar (1.9) |
| 2 | hunch-bkb (2.4), hunch-q8p and hunch-zxz (2.2) | skeleton (2.0); marker lexer (2.1); `pick --from` (2.3); contract (2.5) |
| 3 | hunch-q8p continued | recipe engine and shipped recipes; `commit`, `file`, `dir`, `tool`; contract with `docs/guide/kinds.md` |
| 4 | hunch-qxn | contract |

Close as superseded: hunch-k6s, hunch-x36, hunch-4z4, and hunch-55y (records, the scorer and
abstention per record are beads 1.1, 1.2 and 1.5). Close as obsolete with the argument pass,
in bead 1.4: hunch-gjf, hunch-eil, hunch-u07, hunch-zss.

Older open beads that share files with this plan are ordered behind it:

| Bead | Waits for | Reason |
|:---|:---|:---|
| hunch-bec (deadlines and retries) | bead 1.2 | both write `src/jev/client.rs` |
| hunch-glz (keyless quota) | bead 1.2 | the same file; 1.2 delivers the `rate_limit_day` rule |
| hunch-60l (README voice) | the Phase 3 contract bead, the last one | `README.md` |
| hunch-4ow (`demo.gif`) | bead 1.9 | it records from `demo.tape` |
| hunch-lpp (evaluation) | release 0.7.0 | not a child of the epic |

The epic closes when its children close. No release bead requires the epic closed.

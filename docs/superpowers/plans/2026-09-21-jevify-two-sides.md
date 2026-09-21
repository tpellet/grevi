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

1. **Selection.** Every record a verb prints is a record of its input, byte for byte. Every
   value `fill` substitutes is a handle that a lister printed or the caller wrote.
2. **Time is the cost.** A judgment is almost free; a round trip is not. A verb asks everything
   it can in one round, in parallel, and uses at most two rounds.
3. **Asymmetric doubt.** `filter` keeps what it is unsure about: a dropped record costs the
   whole input. `fill` and `pick` abstain: a wrong act costs more than no act.
4. **Use the tools that exist.** jevify does not do what `jq`, `cut`, `grep`, `head`, `tee` and
   `cd` do. It reads lines and gives lines back, so they all compose with it.
5. **One role for stdin.** A call never reads stdin for two purposes.

Removed, with no shim, in Phase 1 (release 0.5.0): `why -- CMD`, the verb name `run`,
`run --yes`, `run --exec`, the `run` argument pass (`src/args.rs`, `manpage::parse_flags`).
`init` stays: its `,` alias and its `command_not_found` hook call `jevify route`, which only
prints (`src/cmd/agent.rs:188-203` call `jevify run` today).

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
- Every other byte of every argument passes through unchanged. Argv is `OsString` end to end.
- After `--` every token belongs to the command, a second `--` included.

## 2. Architecture

New files:

| File | Contents | Tests |
|:---|:---|:---|
| `src/marker.rs` | Pure lexer: `parse(&[OsString]) -> Result<Vec<Arg>, MarkerError>`; `substitute`. No I/O. | inline table tests |
| `src/records.rs` | The record model of every output verb and of `@{-:…}`, over raw bytes. `Record { handle, evidence: String, raw: Range<usize> }` indexes one input buffer. | inline |
| `src/source.rs` | `enumerate(kind, scope) -> Listing { records, total, omitted }`: the coded kinds and the recipe engine (2.2). Listers run an argv, never a shell, under one deadline. | inline + `tests/fill.rs` |
| `src/kinds.jsonl` | The shipped recipes, one JSON object per line, compiled in with `include_str!`. | the recipe table test |
| `src/cmd/fill.rs` | lex → read inputs → enumerate → resolve → substitute → print or `exec`. | `tests/fill.rs` |
| `src/cmd/filter.rs`, `src/cmd/label.rs` | Per-record verbs; the scorer of 2.3 lives in `filter.rs`. | `tests/filter.rs`, `tests/label.rs` |
| `src/save.rs` | The content-addressed saved input of 2.4. | inline |

Changed in place:

| File | Change |
|:---|:---|
| `src/jev/client.rs`, `src/jev/classifier.rs` | `Client::ask_each(records, questions)`, the per-record request of 2.3 |
| `src/cli.rs` | `Fill`, `Route`, `Why` without `cmd`, `Filter`, `Label`, `Pick --from`, `Is` with several statements and `--context` |
| `src/lib.rs` | `VERBS` (`:23`), `command_name` (`:128`); the clap error path reads `args_os` and stops at `--` (`:63` uses `env::args()`); `human` written as bytes |
| `src/cmd/mod.rs` | `Outcome.human: Vec<u8>` (`:15` is `String`); `Outcome.exec: Option<Vec<OsString>>`, the command that `run_cli` becomes after it has written everything else |
| `src/exit.rs` | one variant `JevifyError::Kinded { kind, exit, message, hint, example }` for the new kinds |
| `src/input.rs` | `read_stdin_bytes()` with the same 64 MiB cap and terminal check |
| `src/output.rs` | `shell_quote(&[OsString]) -> Vec<u8>`, moved from `run.rs:312` and written over bytes |
| `src/tournament.rs` | `window` becomes `pub(crate)` (`:55`); `decide(&Ranking)`; the pool rule of 2.1 (`:126-127`) |
| `src/cmd/why.rs` | delete `capture` and the `cmd` argument (`:59-`); the saved input |
| `src/cmd/run.rs` | becomes `route`: delete execution, the confirmation, the argument pass |
| `src/cmd/pick.rs`, `src/cmd/is.rs` | `--from KIND`; several statements, `--context FILE` |
| `src/cmd/agent.rs` | capabilities, robot-docs, `init agents` |
| `src/config.rs` | Phase 0; the save directory |
| `tests/common/mod.rs` | `FakeJev` answers with a probability vector per question |
| Documents | section 6 |

No new dependency. `fill` replaces itself with the command (`CommandExt::exec`, std, safe), so
there is no child to wait for, no signal to forward and no tokio `signal` feature.

### 2.1 `fill`

`W` is the window of the backend: 200 candidates on TypeSafe, 99 on classifier.dev
(`Backend::window`, `src/config.rs:34`). `F = W × (W / 3)`: 13,200 and 3,267.

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
     best of each window **by rank**, all of them, with tier-two evidence for the first 24
     (`MAX_FINALISTS`). A probability is never compared with one from another request
     (`src/tournament.rs:126-127` does that today and keeps 24). With one window, round 2 runs
     only when the ratio failed and the kind has tier-two evidence.
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
7. **Substitute.** A path handle is relative to the prefix it was listed under, so
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
   - `fill` returns the resolved argv to `run_cli`, and `run_cli` calls `exec` as its last act,
     after `meta` is computed and the status lines are written. No destructor of jevify runs
     after `exec`, so everything that must reach the disk is written before it: the answer
     cache entries of this call (`DiskCache::put` writes through a rename and holds no buffer)
     and the stderr lines (stderr is unbuffered).
   - The tokio runtime is current-thread. At `exec` no request is in flight, because every
     marker is resolved; the blocking pool threads vanish with the process image, and none of
     them holds work.
   - The environment and the working directory pass through unchanged. jevify sets no variable
     for the command.
   - Tests use a helper binary (`tests/bin/argv.rs`, new) that prints its argv as NUL-separated
     bytes, reports whether its stdin is at EOF, and exits with the code given in its first
     argument. It proves the exact argv, the stdin rule and the exit code that comes through.
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
| `commit` | `git log -n F --format=… HEAD`, NUL-delimited | full OID | subject | + body, changed paths |
| `file`, `dir` | `git ls-files -co --exclude-standard -z`; outside a work tree a no-follow walk | path relative to the prefix | path | + first lines (`sort::excerpt`) |
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
  configuration directory (`directories::ProjectDirs`, already a dependency). A user recipe
  cannot replace a coded kind or a shipped recipe. A bad line is exit 6 `recipe_invalid` with
  its line number, when that kind is used.
- jevify reads no recipe from the working directory. A cloned repository never adds a command
  that `fill` runs. A user recipe is the user's own command, as an alias is.
- The engine is the `-` path: run `list`, hand the bytes to `records.rs`, get a `Listing`.
  A recipe is `#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Recipe { kind,
  list: Vec<String>, field: Option<usize>, key: Option<String>, ordered: bool }`. With `key`
  the output is read as JSON lines or one array, through `serde_json`; with `field` or
  neither, as lines. `field` and `key` together, an empty `list`, and a `kind` that does not
  match `[a-z][a-z-]*` are `recipe_invalid`.
- The lister runs as a `std::process::Command` on the blocking pool, with stdout and stderr
  piped and read to the end on two threads (`std::thread::scope`), so a full pipe never blocks
  it. `tokio::time::timeout` bounds the wait; on a timeout the child is killed and waited for.
  Output above 64 MiB is `lister_failed`.

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
- Hidden files are listed; `.git/` never is. Tier-two excerpts are never read for a path with a
  dot component, nor for `.env*`, `*.pem`, `*.key`, `id_*`, `*credentials*`, `*secret*`,
  `.netrc`, `.npmrc`, `.envrc`. The status line says `excerpts withheld: N`. Symlinks are not
  followed.

### 2.3 Records and the scorer

`records.rs` reads bytes through `input::read_stdin_bytes()`. `input::read_stdin` decodes
lossily and trims (`src/input.rs:43-49,69`); it stays as it is for `why` and `is`.

- **Splitting**, exactly one of: lines (default), `-0`, `--para`. Two together is exit 2.
- **`--files`**: the record is a path and the evidence is its first lines, so
  `fd -0 | jevify filter -0 --files '…'` works.
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
  On TypeSafe, `ask_each` keeps 20 records per request in one state, as `add.rs:10` does.
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
    record, the form of `add.rs:10`, and unpacks the answers into one `Response` per record.
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
- **The model.** Every `Response` carries `model` (`src/jev/mod.rs:89`), and classifier.dev
  names it per dimension (`src/jev/classifier.rs:217-219`). `Stats.model` keeps the last one
  today (`src/jev/client.rs:278`); it keeps every distinct one. A model whose name does not
  start with `jev` is named in the status line: `answered by ibm-granite/granite-4.0-h-micro,
  not Jev`. `meta.model` lists them.
- **The pace is the backend's.** Keyless: 3,000 decisions a minute and 20,000 a day per IP.
  The verb prints `jevify filter: 10074 records, 3120 distinct, 4 requests` on stderr before
  the first request. A 429 waits and retries (`src/jev/client.rs:418-422`). A daily-quota 429
  ends the verb with exit 4; the printed records are a correct prefix and the last line says
  `answered 3000 of 3120`.
- **The ceiling** is 20,000 distinct records, a day of the free backend: exit 6 `too_many`, and
  the message says to narrow with `grep` or `head`. There is no flag to raise it.
- **`filter` flags:** `-v` keeps the records where the statement is false, `-c` prints the
  count, `--strict` drops the unsure. Unsure records are kept in both directions. Line numbers
  and context come from `grep` over the saved input:
  `grep -n -C2 -F -f <(jevify filter 'x' < build.log) build.log`.

### 2.4 The saved input

`why` and `filter` write the full input, as the bytes they read, to
`<save dir>/outputs/<blake3-16>.log` before the first request, and print `full output: PATH` as
their last stderr line.

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

### Phase 0 — base URL (hunch-ed1)

`JEVIFY_BASE_URL` must be `https`. The TypeSafe backend accepts only `api.typesafe.ai`, the
classifier backend only `classifier.dev`; `127.0.0.1` and `localhost` accept any scheme, for
wiremock. A URL with userinfo is refused. Redirects are not followed.

Writes: `src/config.rs`, `src/jev/client.rs`, `tests/client.rs`,
`docs/guide/configuration.md`, `PRIVACY.md`, `CHANGELOG.md`.
Done when `tests/client.rs` holds: plain http refused, unknown host refused, a TypeSafe key
with `classifier.dev` refused, userinfo refused, a 302 not followed, localhost accepted.

### Phase 1 — the language: cutover, records, `filter`, `is` (0.5.0)

Goal: one grammar, one record model, and the output side an agent uses most.

| Bead | Writes | After |
|:---|:---|:---|
| 1.0 skeleton — new: language skeleton and grammar cutover | the shared files; `src/config.rs` (save dir); stubs `src/records.rs`, `src/save.rs`, `src/cmd/filter.rs`; `src/cmd/why.rs` + `tests/why.rs` (delete `capture`, the `cmd` argument); the `Outcome` change in every `src/cmd/*.rs`; `tests/cli_basics.rs` | Phase 0 |
| 1.1 — new: byte records | `src/records.rs` | 1.0 |
| 1.2 — new: `ask_each` | `src/jev/client.rs`, `src/jev/classifier.rs`, `tests/client.rs`, `tests/common/mod.rs` (the batch responder and the probability vectors) | 1.0 |
| 1.3 — new: saved input | `src/save.rs` | 1.0 |
| 1.4 — new: route is print-only | `src/cmd/run.rs`, `src/manpage.rs`, `tests/run_route.rs`, `tests/run_args.rs` (rewritten in place); removes `mod args;` from `src/lib.rs` | 1.0 |
| 1.5 — hunch-bx6 (filter and the scorer) | `src/cmd/filter.rs`, `tests/filter.rs` | 1.1, 1.2, 1.3 |
| 1.6 — new: `is` with several statements and `--context` | `src/cmd/is.rs`, `tests/is.rs` | 1.0 |
| 1.7 — new: records and save in `why`, records in `pick` | `src/cmd/why.rs`, `tests/why.rs`, `src/cmd/pick.rs`, `tests/pick.rs` | 1.1, 1.3 |
| 1.8 contract — hunch-sb9 and new: 0.5.0 documents | section 6; last, alone: the version | 1.0 |

`is 'a' 'b' 'c'`: one Noul per statement in one request, one verdict line per statement; exit 1
when any is no, else 3 when any is unsure, else 0. One statement behaves as today.

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
distinct records send none; coloured `rg` output comes out byte-identical; an unwritable save
directory gives `not saved`; a daily-quota 429 leaves a correct prefix; an answer from a model
that is not Jev is named in the status line.

### Phase 2 — `fill` (0.6.0)

Goal: one call replaces "list, read, choose, act", for any list an agent can pipe, for
branches, and for an option that depends on text it has not read.

| Bead | Writes | After |
|:---|:---|:---|
| 2.0 skeleton | the shared files (`Fill`, `Pick --from`, the error kinds); stubs `src/marker.rs`, `src/source.rs`, `src/cmd/fill.rs` | Phase 1 |
| 2.1 — new: marker lexer | `src/marker.rs` | 2.0 |
| 2.2 — hunch-q8p (`-`, `branch`) and hunch-zxz (the pool rule) | `src/source.rs`, `src/tournament.rs`, `tests/tournament.rs` | 2.0 |
| 2.3 — new: `pick --from` | `src/cmd/pick.rs`, `tests/pick.rs` | 2.2 |
| 2.4 — hunch-bkb (fill core) | `src/cmd/fill.rs`, `tests/fill.rs` | 2.1, 2.2 |
| 2.5 contract | section 6; the version | 2.0 |

Done when, in this repository, on the default backend:
```
jevify fill --dry-run -- git switch '@{branch:the release automation work}'
git log --format='%H %s' -3000 | jevify fill --field 1 -- git show '@{-:made folder moves atomic}'   # candidates 3000, windows 31
gh pr list --json number,title | jevify fill --dry-run --key number -- gh pr view '@{-:the Windows path fix}'
jevify fill -- git switch '@{branch:a branch that does not exist}'; echo $?      # 3, nothing ran
cat t.md | jevify fill -- true '@{-:x}' '@{flag:--draft:y}'; echo $?              # 2, stdin has two roles
jevify fill -q -- false '@{-:x}' <<< x; echo $?                                  # 1, the command's own code
jevify pick --from branch 'the release automation work'
```
and, as contract tests: the `gh issue create` example of the vision sends one POST for its
three markers; a helper command receives the exact argv; an unsure `flag` and an answer from
another model run nothing (a sentinel file proves it); `F + 1` lines are `too_many` with no
request; 250 lines resolve in two rounds and the finals hold three finalists of every window.

### Phase 3 — kinds

The recipe engine, the shipped recipes, the user's `kinds.jsonl`; `commit`, `file`, `dir`,
`tool`. All in `src/source.rs` and `src/kinds.jsonl`, serial, then `tests/fill.rs`, then the
contract bead with `docs/guide/kinds.md`.

Done when `'@{commit:made folder moves atomic}'` prints a full OID under `--dry-run`;
`'src/cmd/@{file:stages hunks}'` becomes `src/cmd/add.rs`; a tracked `.npmrc` reports
`excerpts withheld: 1` and its content is absent from the captured request; a line appended to
a temporary `kinds.jsonl` makes `'@{widget:…}'` resolve, the same line named `branch` is
`recipe_invalid`, and a `kinds.jsonl` in the working directory is never read; a fake `gh` that
prints "not logged in" gives `lister_failed` with that text.

### Phase 4 — `label` (hunch-qxn)

`label a,b,c`: one Choice per record, the labels plus an internal `NONE`, through `ask_each`.
Output is `LABEL<TAB>RECORD`; an unsure record has the label `?`. Needs Phase 1 only.

Done when `gh issue list | jevify label bug,feature,question | cut -f1 | sort | uniq -c` prints
a histogram and `cut -f2-` of the output is the input byte for byte.

### After the language is stable

Evaluation follows the language, on a surface that no longer moves: a local invocation log;
a held-out set per verb and kind, labelled by someone other than the author; recall of `filter`
at the unsure boundary; how often the right candidate misses the finalists of its window.

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
- `pick -n N` and `why -n N` are "top N". `why -C N` is context lines.
- `is` takes one or more statements (`Cmd::Is { condition: String }` today, `src/cli.rs:110`).

## 5. Tests

- `marker.rs` table: every literal (`@{u}`, `@{-1}`, `HEAD@{2}`, `stash@{0}`, `@{1 day ago}`,
  `@types/node`, `user@host:path`, PowerShell `@{k='v'}`, Python `{user}@{host:>8}`), prefix,
  suffix, flag value, two markers in one argument, `\}`, quotes stripped once, unterminated,
  unknown kind with suggestion, empty body, a marker in `argv[0]`, a non-UTF-8 literal,
  `one` and `flag`, substituted text that holds `@{`, stdin with two roles.
- `FakeJev` gives the winner 0.9 and spreads 0.1 (`tests/common/mod.rs:26-31`), so no tie and
  no `ambiguous` path can be written today. Bead 1.2 adds a probability vector per question and
  the batch responder; `FakeClassifier` is built from it.
- `tests/fill.rs`: dry-run output equals the argv a run receives (a helper prints its argv as
  NUL-separated bytes); abstain prints nothing and runs nothing; 0, 1 and 2 candidates; a tie;
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
  closed stdout; the ceiling; the prefix after a daily-quota 429; `-v`, `-c`, `--strict`;
  `filter A | filter B` equals `filter B | filter A`; `label … | cut -f2-` equals the input;
  `is --context FILE` equals `is < FILE`; the saved path, two saves, the failed save.
- Recipes: every shipped line parses; a bad line and a shadowed kind are `recipe_invalid`; the
  working directory is never searched. Lister tests put fake `git` and `gh` first on the PATH.
- Live tests (ignored by default) for one `branch`, one `one` and one `filter` batch per
  backend. Without a key they are reported as not run.

## 6. The README, the skill and the `AGENTS.md` block

Three documents teach one language to three readers, in one order: the two sides, the verbs,
the marker, the exit codes. Bead 1.8 rewrites all three; each later contract bead adds what its
phase delivers. None names a verb, a flag or a kind the installed release lacks.

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
   beat the right one; the finals' `NONE` and `any` are the guard then.
2. **Agents misquote the marker.** Every damaged marker the lexer can see is exit 2 with the
   corrected spelling. `$x` expanded inside double quotes is invisible; the single-quote habit
   is the guard.
3. **The command's exit code can be 2 to 6.** The `exec` line on stderr says whose code it is.
4. **The free backend answers with another model, or its quota ends.** The model guard, the
   model in every status line, exit 4, a correct prefix.
5. **Saved inputs hold secrets and grow.** 0600 in 0700, `PRIVACY.md`, `--no-save`.
6. **A user recipe runs a command.** Only from the user's configuration directory; it cannot
   shadow a shipped kind; `capabilities` prints its argv.

## 9. Owner decisions

1. `git mv src/cmd/run.rs src/cmd/route.rs`, `tests/run_route.rs` → `tests/route.rs`, and the
   removal of `src/args.rs` and `tests/run_args.rs`. Without permission the files keep their
   names and `src/args.rs` stays on disk outside the module tree.
2. `fill` becomes the command through `exec`. The command's exit code comes through, and exit 7
   leaves `fill`. The alternative, a child with signal forwarding, keeps one exit contract and
   costs the signal code, a dependency feature and four test families.
3. An unsure `flag` abstains.
4. `fill` resolves through windows up to `F` in two rounds; the pool is never cut.
5. Per-record verbs use the batch request of classifier.dev; `fill` refuses an answer that is
   not from Jev.
6. Cut on purpose, listed under Later: `-C DIR`, `--evidence`, the identity re-check, the
   `test`, `script` and `complete` kinds with their consent flag, `filter -e/-n/-A/-B/-C`,
   `label --ordered`, `--max-records`.
7. Saved inputs are content-addressed and never pruned by jevify.

## 10. Beads

| Phase | Existing beads | New beads |
|:---|:---|:---|
| 0 | hunch-ed1 | — |
| 1 | hunch-bx6 (1.5), hunch-sb9 (1.8) | skeleton and cutover (1.0); byte records (1.1); `ask_each` (1.2); saved input (1.3); route is print-only (1.4); `is` statements and `--context` (1.6); records in `why` and `pick` (1.7); 0.5.0 documents (1.8) |
| 2 | hunch-bkb (2.4), hunch-q8p and hunch-zxz (2.2) | skeleton (2.0); marker lexer (2.1); `pick --from` (2.3); contract (2.5) |
| 3 | hunch-q8p continued | recipe engine and shipped recipes; `commit`, `file`, `dir`, `tool`; contract with `docs/guide/kinds.md` |
| 4 | hunch-qxn | contract |

Close as superseded: hunch-k6s, hunch-x36, hunch-4z4. Close as obsolete with the argument pass,
in bead 1.4: hunch-gjf, hunch-eil, hunch-u07, hunch-zss.

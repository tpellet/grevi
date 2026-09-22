# What leaves your machine

jevify sends evidence to the active backend:

| Backend | Selected when | Destination | Authentication |
|:---|:---|:---|:---|
| `typesafe` | a key is set, or `JEVIFY_BACKEND=typesafe` | `https://api.typesafe.ai` | your key |
| `classifier` | no key is set, or `JEVIFY_BACKEND=classifier` | `https://classifier.dev` | none |

`JEVIFY_BASE_URL` accepts only HTTPS at the active backend's host, on port 443. Host comparison
ignores case; trailing dots, other hosts and ports, userinfo and malformed URLs are rejected
before requests. Blank values use the default. For local testing, `localhost` and `127.0.0.1`
accept any scheme and port, without userinfo; these endpoints receive the same evidence and,
on TypeSafe, the bearer key. Inference, prewarm and health requests never follow redirects.

`meta.backend` names the API. `meta.model` names the answering model, with several models joined
by `", "`. A free-backend response is service-controlled and can name a different model.

## Evidence per verb

| Verb | Sent | Not sent |
|:---|:---|:---|
| `fill` | descriptions, context of `one` and `flag`, and candidates' evidence, with best-effort redaction: branch names and subjects, commit subjects and, for finalists, bodies and changed paths, file paths and, for finalists, first lines, tool names and summaries, the whole line of a recipe's listing | literal command arguments outside markers are not semantic evidence; the caller-written command executes locally; the content of a withheld file |
| `pick --from` | the same evidence as `fill` for the kind | no user command executes |
| `pick` | description and distinct stdin records, clipped to 200–2,000 characters per selection item | unselected evidence beyond the clipping budget |
| `pick --files` | description, stdin paths, masked excerpts of at most 24 finalists | withheld file contents; other files not listed on stdin |
| `filter` | statement and distinct record evidence; with `--files`, stdin paths and eligible file excerpts | file content beyond excerpts, or content withheld by the path rules; the path of a file that cannot be read, which is unsure without a request |
| `label` | the labels and distinct record evidence, redacted on a best-effort basis; with `--files`, stdin paths and eligible file excerpts | file content beyond excerpts, or content withheld by the path rules; the path of a file that cannot be read, which is `?` without a request; nothing is saved |
| `why` | filtered stdin log, at most 4,000 selected lines, each clipped | lines filtered out locally |
| `is` | statements and complete supported context from stdin or `--context FILE` | oversized context: it abstains before inference |
| `route` | intent, installed tool names and summaries, man-page excerpts of at most 12 finalists | directory file contents, shell history, environment values |
| `add` | topic and complete unstaged hunks of tracked files, header plus body, at most 3,000 characters each | untracked files and content outside the diff; oversized hunks are rejected |
| `sort` | eligible file and folder names, first 2,000 characters of file text | hidden files, symlink entries, files below the source directory, content beyond excerpts |
| `health` | reachability request and TypeSafe bearer key when needed | user text |
| `capabilities`, `robot-docs`, `init` | nothing | all local data |

PDF excerpts use text from the first two pages when `pdftotext` is installed, clipped to 2,000
characters. File excerpts do not establish a whole-document verdict.

`fill` starts the command the caller wrote. `--dry-run` resolves and prints argv without starting
it; both forms send evidence. `branch`, `commit`, `file` and `dir` run local Git listers; `tool`
reads the PATH and the man index; `pr`, `issue`, `ci-run`, `stash`, `process`, `container` and
`pod` run the owning tool's listing; `-` reads supplied candidates; `one` and `flag` read
context. `jevify capabilities --json` prints the argv of every lister. A user recipe in
`kinds.jsonl` under `JEVIFY_CONFIG_DIR` is the user's own command, as an alias is: jevify reads
no recipe from a repository, so a clone never adds a command that `fill` runs. Every lister runs
with stdin at `/dev/null`, `GH_PROMPT_DISABLED=1`, `GIT_TERMINAL_PROMPT=0` and `NO_COLOR=1`,
under one 20 s deadline. `fill` and `pick --from` do not save raw inputs. Execution requires
every answering model to be Jev; a missing model name is `unknown` and refuses execution.

`--files` is a boolean on `pick`, `filter` and `label`, with paths supplied by the caller on stdin;
the `file` kind lists paths itself, hidden ones included, and applies the same rule to its
finalists. Paths of hidden files leave the machine as names; their content does not.
Before reading an excerpt, jevify withholds any path whose written components:

- start with `.` (except navigation `.` and `..`), or with `id_`;
- end with `.pem` or `.key`;
- contain `credentials` or `secret`.

These checks are case-sensitive. Symlink file entries also receive no excerpt. The path remains
a candidate and can leave the machine as a name; a withheld excerpt is not a withheld path.
The stderr status reports `excerpts withheld: N`. The checks are not a filesystem sandbox.

Before sending, jevify masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`,
JWTs) in semantic state and questions as `[REDACTED]`. Opaque option IDs remain stable.
Masking is best effort, not a guarantee: do not send secrets to a backend you do not trust.
jevify never prints or logs your API key. Use `TYPESAFE_API_KEY_FILE=/path/to/key`.

## Two stores

The base directory is `JEVIFY_CACHE_DIR`, or the platform cache directory's `jevify` directory:
`~/Library/Caches/jevify` on macOS; `$XDG_CACHE_HOME/jevify` or `~/.cache/jevify` on Linux.

| Store | Contents | Retention | Disable |
|:---|:---|:---|:---|
| Answer cache | answers and probabilities, keyed by a hash of the redacted request | answers expire after seven days; expiry does not reclaim files | `--no-cache` or `JEVIFY_NO_CACHE=1` |
| Saved inputs, `outputs/<blake3-16>.log` | full raw input bytes, secrets included | never pruned by jevify | `--no-save` on `why` and `filter` |

Only `why` and `filter` save inputs; `label` saves nothing. They save before the first inference request, with directory
mode 0700 and file mode 0600. Identical input has the same content-addressed path. `--no-cache`
does not disable saving. Stderr and `data.saved_input` name the file; a failed or skipped save
reports `full output: not saved (REASON)` and sets `data.complete=false`.

The answer cache never crosses backend, endpoint or decision-contract versions. Tool inventory
and `sort` recovery journals also use the cache directory. Recovery journals retain local
absolute path bytes and file identity for undo; they are not sent to the model. Preserve journals
needed for recovery. Concurrent replacement of source files while sorting is unsupported.

## Backend limits and handling

`filter` and `label` batch up to 1,000 records per request on classifier.dev, each judged alone. With
TypeSafe, 20 records share one request state; each question names its record, but independence
is not claimed. A `rate_limit_day` HTTP 429 returns exit 4, `daily quota of the free backend
reached`, with no retry. Human output already emitted before a later failure can be a prefix.

Service policies: [TypeSafe](https://docs.typesafe.ai/legal),
[classifier.dev privacy](https://classifier.dev/privacy) and
[terms](https://classifier.dev/terms). Free-backend requests need no account, but are not private:
the service sees your IP and the `jevify/<version>` user agent. Select TypeSafe if your text must
not go to the free backend; both choices send evidence off the machine.

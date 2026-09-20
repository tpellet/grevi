# What leaves your machine

grevi sends requests to one API, the active backend's (`GREVI_BASE_URL` overrides it):

| Backend | When | Where the text goes | Authentication |
|---|---|---|---|
| `typesafe` | a key is set, or `GREVI_BACKEND=typesafe` | the TypeSafe API, `https://api.typesafe.ai` | your key |
| `classifier` | no key is set | classifier.dev, `https://classifier.dev`, which runs the same Jev model and serves it free | none; no key, no account, no cookie |

Which one answered is in `meta.backend` of the JSON envelope and in `grevi health`. The table
below is the same for both.

| Verb | Sent | Never sent |
|---|---|---|
| pick | your intent and the stdin lines (each clipped to 200–2,000 characters; ≤ 20,000 lines) | anything else |
| why | your stdin (or the `--` command's output) after local filtering (≤ 4,000 lines, each clipped) | lines filtered out locally |
| is | your condition and stdin (≤ ~96k chars on `typesafe`, ≤ 30k on `classifier`, head+tail) | — |
| run | your intent, names and one-line descriptions of installed tools, man-page excerpts of ≤ 12 finalists, the chosen tool's option list, and only those file names in the current directory that contain a word of your request (never contents) | other file names, file contents, environment, history |
| add | your topic and each unstaged hunk of tracked files (header + body, clipped to 3,000 characters) | untracked files, file contents outside the diff |
| sort | the names of the files directly in the directory, the first 2,000 characters of each text file (or of a PDF's first two pages via `pdftotext`, when installed), and the folder names under the root | hidden files, files in sub-folders, the rest of each file, binary contents |

Before sending, grevi masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`,
JWTs) as `[REDACTED]`. This is best effort, not a guarantee: do not pipe secrets into grevi.

Answers are cached on disk in your cache directory (`GREVI_CACHE_DIR`), keyed by a hash of the
request, for 7 days; the cache holds answers (option ids and probabilities), not your text.
`--no-cache` or `GREVI_NO_CACHE=1` disables it; a cache entry never crosses from one backend to
the other. grevi never logs or prints your key.

Each service's own data handling: TypeSafe, https://docs.typesafe.ai/legal; classifier.dev,
https://classifier.dev/privacy and https://classifier.dev/terms. On classifier.dev the requests
are anonymous but not private: they are rate-limited per IP, and grevi identifies itself with a
`grevi/<version>` user agent. If your text must not reach a third party you did not sign up
with, set a TypeSafe key or `GREVI_BACKEND=typesafe`.

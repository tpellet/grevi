# What leaves your machine

hunch sends requests only to the TypeSafe API (`HUNCH_BASE_URL`), authenticated with your key.

| Verb | Sent | Never sent |
|---|---|---|
| pick | your intent and the stdin lines (each clipped to 200–2,000 characters; ≤ 20,000 lines) | anything else |
| why | your stdin (or the `--` command's output) after local filtering (≤ 4,000 lines, each clipped) | lines filtered out locally |
| is | your condition and stdin (≤ ~96k chars, head+tail) | — |
| run | your intent, names and one-line descriptions of installed tools, man-page excerpts of ≤ 12 finalists, the chosen tool's option list, and only those file names in the current directory that contain a word of your request (never contents) | other file names, file contents, environment, history |
| add | your topic and each unstaged hunk of tracked files (header + body, clipped to 3,000 characters) | untracked files, file contents outside the diff |
| sort | the names of the files directly in the directory, the first 2,000 characters of each text file (or of a PDF's first two pages via `pdftotext`, when installed), and the folder names under the root | hidden files, files in sub-folders, the rest of each file, binary contents |

Before sending, hunch masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`,
JWTs) as `[REDACTED]`. This is best effort, not a guarantee: do not pipe secrets into hunch.

Answers are cached on disk in your cache directory (`HUNCH_CACHE_DIR`), keyed by a hash of the
request, for 7 days; the cache holds answers (option ids and probabilities), not your text.
`--no-cache` or `HUNCH_NO_CACHE=1` disables it. hunch never logs or prints your key.
TypeSafe's own data handling: https://docs.typesafe.ai/legal

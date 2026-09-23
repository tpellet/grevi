#!/usr/bin/env python3
"""Dump message-free diffs for the selected commits.

Enforcement: git show runs with --format= so no subject, body or author line is
ever written into the file the description author reads. The patch is cut at 400
lines and each line at 300 characters.

Selection: an anchor every 10th commit from HEAD; from each anchor, the nearest
commit (searching forward) that changes something other than .beads/issues.jsonl,
so the targets are substantive commits and not tracker churn.
"""
import pathlib
import subprocess

ROOT = pathlib.Path("/Users/tpellet/Projects/jevify")
OUT = ROOT / "evals/commit-attractor/diffs"
OUT.mkdir(parents=True, exist_ok=True)


def git(*args):
    return subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=True
    ).stdout


shas = git("rev-list", "HEAD").split()


def substantive(sha):
    names = [
        n
        for n in git("show", "--format=", "--name-only", sha).split("\n")
        if n.strip()
    ]
    return any(n != ".beads/issues.jsonl" for n in names)


chosen = {}
for anchor in range(5, len(shas) + 1, 10):
    for k in range(anchor, min(anchor + 6, len(shas) + 1)):
        sha = shas[k - 1]
        if sha not in chosen.values() and substantive(sha):
            chosen[k] = sha
            break
# the commit the bead names, at its own position
i332 = shas.index("332191a1639f5dc784e976fe4d404ee9a564e272") + 1
chosen[i332] = shas[i332 - 1]

lines = []
for idx in sorted(chosen):
    sha = chosen[idx]
    short = git("rev-parse", "--short", sha).strip()
    stat = git("show", "--format=", "--stat", sha)
    patch = "\n".join(
        l if len(l) <= 300 else l[:300] + " \u2026[line truncated at 300 characters]"
        for l in git("show", "--format=", "--patch", sha).splitlines()[:400]
    )
    (OUT / f"{idx:03d}-{short}.diff").write_text(
        f"index-from-HEAD: {idx}\nshort: {short}\n"
        f"--- stat ---\n{stat}\n--- patch (first 400 lines) ---\n{patch}\n"
    )
    lines.append(f"{idx}\t{sha}\t{short}")

(ROOT / "evals/commit-attractor/selection.tsv").write_text("\n".join(lines) + "\n")
print(len(lines), "diffs written")

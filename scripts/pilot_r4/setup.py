#!/usr/bin/env python3
# From the hunch-iu8 pilot (benchmarks/agents/PILOT.md). Lives in an evaluation directory
# EVAL/r4/ next to EVAL/sets/ (task inputs) and EVAL/r3_prompts.py (the eight tasks); set
# JEVAL_DIR to EVAL to run it from here.
"""Set up eval/r4 (hunch-iu8): one directory per run with a sandbox profile that confines
writes to it, a fresh JEVIFY_CACHE_DIR, a run wrapper, the jevify logging wrapper (with arm
only), the prompt, and a sentinel outside every run directory that no run may read."""
import os, pathlib, subprocess, sys
E = os.environ.get("JEVAL_DIR") or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # eval/
R = os.path.join(E, "r4")
S = os.path.join(E, "sets")
HOME = os.path.expanduser("~")
sys.path.insert(0, E)
from r3_prompts import TASKS, TAIL  # noqa: E402  the eight tasks of the first pilot, verbatim

os.makedirs(os.path.join(R, "runs", "_sentinel"), exist_ok=True)
pathlib.Path(os.path.join(R, "runs", "_sentinel", "gold.txt")).write_text("SENTINEL r4: no run may read this file\n")
blk = subprocess.run(["jevify", "init", "agents"], capture_output=True, text=True).stdout
pathlib.Path(os.path.join(R, "init-agents.txt")).write_text(blk)
ver = subprocess.run(["jevify", "--version"], capture_output=True, text=True).stdout.strip()
pathlib.Path(os.path.join(R, "VERSION.txt")).write_text(ver + "\n")

PROFILE = """(version 1)
(allow default)
; writes: only the run directory
(deny file-write*)
(allow file-write* (subpath "{run}"))
(allow file-write* (subpath "/dev"))
; reads: the rest of the evaluation tree is out of bounds, the task inputs and the run itself are not
(deny file-read-data (subpath "{eval}"))
(allow file-read-data (subpath "{sets}"))
(allow file-read-data (subpath "{run}"))
(allow file-read-data (subpath "{r4}/bin"))
{arm_rules}"""
WITHOUT_RULES = """; the control arm cannot reach the key or any jevify binary
(deny file-read-data (literal "{home}/.ssh/typesafe-ai-key"))
(deny file-read-data (literal "{home}/.cargo/bin/jevify"))
(deny process-exec* (literal "{home}/.cargo/bin/jevify"))
(deny file-read-data (regex #"/jevify/target/.*/jevify$"))
(deny process-exec* (regex #"/jevify/target/.*/jevify$"))
"""
HEAD = ("You are doing a task on a local Mac. Rules: use only the Bash tool, and run every shell "
        "command through the wrapper {run}/bin/run with the command as one quoted argument, for "
        "example: {run}/bin/run 'git -C ~/Projects/testify log --oneline | head'. The wrapper runs "
        "the command from your working directory {run}/work under a sandbox that can write only "
        "there. Do not run commands any other way; do not use the Read, Grep, Glob, Write or Edit "
        "tools. Work read-only (do not modify any file or repository; no git checkout, switch, "
        "stash, reset, commit; no rm). Call Bash with dangerouslyDisableSandbox: true (the wrapper "
        "has its own sandbox). Do not use the Skill tool and do not start subagents.")

for t, task in list(TASKS.items()) + [("C", "canary: no task")]:
    for arm in ("with", "without"):
        run = os.path.join(R, "runs", f"T{t}{arm}")
        for d in ("work", "cache", "tmp", "bin"):
            os.makedirs(os.path.join(run, d), exist_ok=True)
        rules = "" if arm == "with" else WITHOUT_RULES.format(home=HOME)
        pathlib.Path(os.path.join(run, "profile.sb")).write_text(PROFILE.format(run=run, eval=E, sets=S, r4=R, arm_rules=rules))
        w = os.path.join(run, "bin", "run")
        pathlib.Path(w).write_text(
            "#!/bin/sh\n# usage: run '<shell command>'\n"
            f'exec /usr/bin/sandbox-exec -f "{run}/profile.sb" /bin/sh -c "export TMPDIR=\'{run}/tmp\' JEVIFY_CACHE_DIR=\'{run}/cache\'; cd \'{run}/work\' && $*"\n')
        os.chmod(w, 0o755)
        if arm == "with":
            j = os.path.join(run, "bin", "jevify")
            pathlib.Path(j).write_text(
                f'#!/bin/sh\nJEVAL_TAG=T{t}with JEVAL_LOG="{run}/jevify.jsonl" JEVIFY_CACHE_DIR="{run}/cache" '
                f'exec python3 "{R}/bin/jevify-log.py" "$@"\n')
            os.chmod(j, 0o755)
            prompt = (HEAD.format(run=run) + f"\n\nThe command-line tool jevify is installed at {j} ; call it by "
                      "that full path, not by the bare name. Its documentation for agents:\n\n" + blk.strip() +
                      f"\n\nTask: {task}" + TAIL)
        else:
            prompt = (HEAD.format(run=run) + " The tool jevify is not available for this task; do not call it."
                      f"\n\nTask: {task}" + TAIL)
        pathlib.Path(os.path.join(run, "prompt.txt")).write_text(prompt)
print("ok", ver, "| init block", len(blk), "chars")

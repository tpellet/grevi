#!/usr/bin/env python3
# From the hunch-iu8 pilot (benchmarks/agents/PILOT.md). Lives in an evaluation directory
# EVAL/r4/ next to EVAL/sets/ (task inputs) and EVAL/r3_prompts.py (the eight tasks); set
# JEVAL_DIR to EVAL to run it from here.
"""Logging wrapper around the installed jevify for one run directory (r4).

Every call is passed through unchanged. When the agent did not ask for a machine format, a
shadow call with `--json` (and `--dry-run` for fill/add) runs first on the same stdin so the
envelope's cost figures (requests, questions, cache hits, tokens, cost estimate) are recorded;
the agent's own call then replays the shadow's answers from the run's fresh cache. One JSON line
per agent call goes to $JEVAL_LOG. The key is never read here: jevify reads TYPESAFE_API_KEY_FILE.
"""
import json, os, shutil, subprocess, sys, time

REAL = shutil.which("jevify") or os.path.expanduser("~/.cargo/bin/jevify")
LOG = os.environ["JEVAL_LOG"]
args = sys.argv[1:]
# jevify reads the key itself; this wrapper only names the file.
os.environ["TYPESAFE_API_KEY_FILE"] = os.path.expanduser("~/.ssh/typesafe-ai-key")

pre = args[: args.index("--")] if "--" in args else args
verb = next((a for a in pre if not a.startswith("-")), None)
flags = [a for a in pre if a.startswith("-")]
machine = any(f in ("--json", "--robot", "--jsonl", "--toon") or f.startswith("--format") for f in flags)
reads_stdin = verb in ("pick", "filter", "label", "why", "is", "add") and "--from" not in flags \
    and not (verb == "is" and "--context" in flags)
data = None
if reads_stdin and not sys.stdin.isatty():
    data = sys.stdin.buffer.read()


def run(argv, stdin_bytes):
    t0 = time.time()
    if stdin_bytes is None:
        p = subprocess.run([REAL] + argv, stdin=subprocess.DEVNULL if reads_stdin is False else sys.stdin,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    else:
        p = subprocess.run([REAL] + argv, input=stdin_bytes, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return p, int((time.time() - t0) * 1000)


def meta_of(stdout):
    try:
        j = json.loads(stdout.decode("utf-8", "replace"))
    except Exception:
        return None
    m = j.get("meta") or {}
    t = m.get("telemetry") or {}
    u = m.get("usage") or {}
    return {"ok": j.get("ok"), "exit_code": j.get("exit_code"), "error_kind": (j.get("error") or {}).get("kind"),
            "model": m.get("model"), "requests": m.get("requests"), "cache_hits": m.get("cache_hits"),
            "questions": t.get("semantic_questions"), "semantic_calls": (t.get("semantic_calls") or {}).get("attempted"),
            "input_tokens": m.get("input_tokens"), "cost_usd": m.get("cost_usd"),
            "cost_estimate": t.get("cost_estimate"), "tokens": u.get("tokens"), "elapsed_ms": m.get("elapsed_ms")}


rec = {"ts": round(time.time(), 3), "tag": os.environ.get("JEVAL_TAG", ""), "verb": verb, "flags": flags,
       "argv": args, "stdin_bytes": None if data is None else len(data), "shadow": None}
if machine:
    p, ms = run(args, data)
    rec.update(exit=p.returncode, elapsed_ms=ms, meta=meta_of(p.stdout))
else:
    sh = list(args)
    ins = sh.index("--") if "--" in sh else len(sh)
    extra = ["--json"] + (["--dry-run"] if verb in ("fill", "add") and "--dry-run" not in flags else [])
    sh = sh[:ins] + extra + sh[ins:]
    sp, sms = run(sh, data)
    rec["shadow"] = {"argv": sh, "exit": sp.returncode, "elapsed_ms": sms, "meta": meta_of(sp.stdout)}
    p, ms = run(args, data)
    rec.update(exit=p.returncode, elapsed_ms=ms, meta=meta_of(p.stdout) if p.stdout[:1] == b"{" else None)
try:
    sys.stdout.buffer.write(p.stdout); sys.stdout.flush()
    sys.stderr.buffer.write(p.stderr); sys.stderr.flush()
except BrokenPipeError:
    pass
err = p.stderr.decode("utf-8", "replace")
rec["stderr_status"] = [l for l in err.splitlines() if l.startswith("jevify")][-4:]
with open(LOG, "a") as f:
    f.write(json.dumps(rec) + "\n")
sys.exit(p.returncode)

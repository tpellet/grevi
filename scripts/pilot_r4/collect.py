#!/usr/bin/env python3
# From the hunch-iu8 pilot (benchmarks/agents/PILOT.md). Lives in an evaluation directory
# EVAL/r4/ next to EVAL/sets/ (task inputs) and EVAL/r3_prompts.py (the eight tasks); set
# JEVAL_DIR to EVAL to run it from here.
"""Per-run metrics for r4 from the subagent transcripts (turns, tool uses, tokens, wall time,
commands, wrapper compliance) joined with each run's jevify.jsonl (requests, questions, cache
hits, cost). Reads runs/<run>/agent-id (written by the lead after each Agent call). Writes
runs.jsonl and one plain-text transcript per run."""
import json, os, pathlib, re, shutil, sys
R = os.environ.get("JEVAL_R4") or os.path.dirname(os.path.abspath(__file__))
SUB = os.environ.get("JEVAL_SUBAGENTS") or os.path.expanduser("~/.claude/projects/-Users-tpellet-Projects-jevify/0d240163-be11-4301-88c6-a24db1242855/subagents")
os.makedirs(os.path.join(R, "transcripts"), exist_ok=True)


def ts(m):
    t = m.get("timestamp")
    if not t:
        return None
    from datetime import datetime
    return datetime.fromisoformat(t.replace("Z", "+00:00")).timestamp()


out = []
for run in sorted(os.listdir(os.path.join(R, "runs"))):
    d = os.path.join(R, "runs", run)
    idf = os.path.join(d, "agent-id")
    if not os.path.exists(idf):
        continue
    aid = pathlib.Path(idf).read_text().strip()
    src = os.path.join(SUB, f"agent-{aid}.jsonl")
    if not os.path.exists(src):
        out.append({"run": run, "missing": aid}); continue
    shutil.copy(src, os.path.join(R, "transcripts", f"{run}.jsonl"))
    turns = tools = 0; tok_in = tok_out = 0; cmds = []; text = []; t0 = t1 = None; non_bash = 0
    with open(src) as fh:
        lines = fh.readlines()
    for line in lines:
        try:
            m = json.loads(line)
        except Exception:
            continue
        t = ts(m)
        if t:
            t0 = t if t0 is None else min(t0, t); t1 = t if t1 is None else max(t1, t)
        msg = m.get("message") or {}
        if m.get("type") == "assistant":
            turns += 1
            u = msg.get("usage") or {}
            tok_in += (u.get("input_tokens") or 0) + (u.get("cache_read_input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0)
            tok_out += u.get("output_tokens") or 0
            for c in msg.get("content") or []:
                if c.get("type") == "tool_use":
                    tools += 1
                    inp = c.get("input") or {}
                    if c.get("name") != "Bash":
                        non_bash += 1
                    cmd = inp.get("command") or json.dumps(inp)[:300]
                    cmds.append((c.get("name"), cmd)); text.append(f"TOOL {c.get('name')}: {cmd}")
                elif c.get("type") == "text":
                    text.append("ASSISTANT: " + c["text"])
        elif m.get("type") == "user" and isinstance(msg.get("content"), list):
            for c in msg["content"]:
                if c.get("type") == "tool_result":
                    body = c.get("content")
                    if isinstance(body, list):
                        body = " ".join(x.get("text", "") for x in body if isinstance(x, dict))
                    text.append(f"RESULT ({len(str(body))} chars): " + str(body)[:1500])
    wrapped = sum(1 for n, c in cmds if n == "Bash" and c.lstrip().startswith(d + "/bin/run"))
    bash = sum(1 for n, _ in cmds if n == "Bash")
    calls = []
    jl = os.path.join(d, "jevify.jsonl")
    if os.path.exists(jl):
        with open(jl) as fh:
            jlines = fh.readlines()
        for line in jlines:
            j = json.loads(line)
            meta = j.get("meta") or (j.get("shadow") or {}).get("meta") or {}
            calls.append({"verb": j["verb"], "flags": j["flags"], "exit": j["exit"], "elapsed_ms": j["elapsed_ms"],
                          "shadow": j["shadow"] is not None, "requests": meta.get("requests"), "questions": meta.get("questions"),
                          "cache_hits": meta.get("cache_hits"), "input_tokens": meta.get("input_tokens"),
                          "cost_usd": meta.get("cost_usd"), "cost_complete": (meta.get("cost_estimate") or {}).get("complete"),
                          "error_kind": meta.get("error_kind")})
    pathlib.Path(R, "transcripts", f"{run}.txt").write_text("\n".join(text))
    out.append({"run": run, "agent": aid, "turns": turns, "tool_uses": tools, "bash": bash, "wrapped": wrapped, "non_bash_tools": non_bash,
                "input_tokens_total": tok_in, "output_tokens": tok_out, "wall_s": round(t1 - t0, 1) if t0 and t1 else None,
                "jevify_calls": len(calls), "calls": calls,
                "requests": sum(c["requests"] or 0 for c in calls), "questions": sum(c["questions"] or 0 for c in calls),
                "cache_hits": sum(c["cache_hits"] or 0 for c in calls), "cost_usd": round(sum(c["cost_usd"] or 0 for c in calls), 5),
                "jev_input_tokens": sum(c["input_tokens"] or 0 for c in calls)})
with open(os.path.join(R, "runs.jsonl"), "w") as f:
    for r in out:
        f.write(json.dumps(r) + "\n")
for r in out:
    print(json.dumps({k: v for k, v in r.items() if k != "calls"}))
    for c in r.get("calls", []):
        print("   ", json.dumps(c))

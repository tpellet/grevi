#!/usr/bin/env python3
"""Paired real Codex episodes; artifacts are private and append-only.

This runner is exploratory: Codex's workspace sandbox restricts writes, not reads.
It does not establish treatment isolation, exact model snapshots, or billed cost.
No confirmatory claims are supported by this runner alone.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import selectors
import shutil
import signal
import subprocess
import time


COMMON_PROMPT = (
    "Complete the task in TASK.md. Use the available tools. Verify the requested "
    "result. If the evidence is insufficient, say so. Follow the task's action "
    "permissions. Return the required result and evidence. Never delete files. "
    "Stay within this task directory; do not inspect other episodes or graders."
)
LIMITATIONS = [
    "Filesystem reads and A-arm grevi availability are not host-isolated.",
    "Tool/output budgets are detected from CLI events; an in-flight tool may finish.",
    "Requested model is recorded; exact returned model snapshot may be unavailable.",
    "Provider cost and Jev usage remain unavailable unless separately instrumented.",
    "Raw artifacts are private and require review/redaction before publication.",
]
TOOL_TYPES = {"command_execution", "mcp_tool_call", "web_search", "file_change"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def relative_path(value):
    path = Path(value)
    if not value or path.is_absolute() or ".." in path.parts or path == Path("."):
        raise ValueError(f"Unsafe fixture path: {value!r}")
    if path.parts[0] in {".git", "TASK.md", "GREVI_SKILL.md", ".grevi-bin"}:
        raise ValueError(f"Reserved fixture path: {value!r}")
    return path


def validate_tasks(tasks):
    if not isinstance(tasks, list) or not tasks:
        raise ValueError("Taskset must be a nonempty list")
    ids = set()
    for task in tasks:
        for key in ("id", "family_id", "endpoint", "stratum", "prompt"):
            if not isinstance(task.get(key), str) or not task[key]:
                raise ValueError(f"Missing nonempty {key}")
        if task["id"] in ids:
            raise ValueError("Duplicate task ID")
        ids.add(task["id"])
        if not isinstance(task.get("files"), dict):
            raise ValueError("files must map relative paths to text")
        for field in ("initial_files", "staged_files", "files"):
            for name, contents in task.get(field, {}).items():
                relative_path(name)
                render(contents)
        for name in task.get("directories", []):
            relative_path(name)
        expected = task.get("expected")
        if not isinstance(expected, dict) or not expected:
            raise ValueError("Every task needs an objective expected rubric")
        if set(expected) - {"answer", "files", "absent", "unchanged", "index"}:
            raise ValueError("Unknown expected rubric")
        for key in ("files", "index", "absent", "unchanged"):
            for name in expected.get(key, {}):
                relative_path(name)
        if "index" in expected and not task.get("init_git"):
            raise ValueError("Index grading requires init_git")


def render(contents):
    if isinstance(contents, str):
        return contents
    if not isinstance(contents, dict) or set(contents) != {"segments"}:
        raise ValueError("Fixture must be text or segments")
    parts = []
    for segment in contents["segments"]:
        repeat = segment.get("repeat", 1)
        if not isinstance(segment.get("text"), str) or type(repeat) is not int or not 0 < repeat <= 1000000:
            raise ValueError("Invalid fixture segment")
        parts.append(segment["text"] * repeat)
    text = "".join(parts)
    if len(text.encode()) > 100000000:
        raise ValueError("Fixture exceeds 100 MB")
    return text


def materialize(task, root):
    root.mkdir(parents=True)
    for name in task.get("directories", []):
        (root / relative_path(name)).mkdir(parents=True, exist_ok=True)
    git = ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
           "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]
    if task.get("init_git"):
        subprocess.run(git + ["init", "-q"], cwd=root, check=True, timeout=10)
    for field in ("initial_files", "staged_files", "files"):
        names = []
        for name, contents in task.get(field, {}).items():
            path = root / relative_path(name)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(render(contents))
            names.append(name)
        if names and field != "files":
            if not task.get("init_git"):
                raise ValueError("Initial/staged files require init_git")
            subprocess.run(git + ["add", "--", *names], cwd=root, check=True, timeout=10)
            if field == "initial_files":
                subprocess.run(git + ["commit", "-qm", "Synthetic fixture baseline"],
                               cwd=root, check=True, timeout=10)
    (root / "TASK.md").write_text(task["prompt"])


def schedule(tasks, repetitions, seed):
    rng = random.Random(seed)
    blocks = {}
    for index, task in enumerate(tasks):
        for replicate in range(repetitions):
            blocks.setdefault(task["endpoint"], []).append((index, replicate))
    pairs = []
    for block in sorted(blocks):
        entries = blocks[block]
        rng.shuffle(entries)
        first = rng.choice(["AB", "BA"])
        for i, (index, replicate) in enumerate(entries):
            pairs.append({"task_index": index, "replicate": replicate,
                          "order": first if i % 2 == 0 else first[::-1]})
    rng.shuffle(pairs)
    return pairs


def hashes(root):
    result = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if relative.parts[0] in {".git", ".grevi-bin"}:
            continue
        if path.is_symlink():
            result[str(relative)] = {"symlink": os.readlink(path)}
        elif path.is_file():
            result[str(relative)] = digest(path.read_bytes())
    return result


def read_local(root, name):
    path = root / relative_path(name)
    if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("Graded path escapes task directory")
    return path.read_text()


def grade(task, root, before):
    checks = {}
    expected = task["expected"]
    if "answer" in expected:
        try:
            checks["answer"] = json.loads(read_local(root, "answer.json")) == expected["answer"]
        except (OSError, ValueError):
            checks["answer"] = False
    for name, contents in expected.get("files", {}).items():
        try:
            checks[f"file:{name}"] = read_local(root, name) == render(contents)
        except (OSError, ValueError):
            checks[f"file:{name}"] = False
    for name in expected.get("absent", []):
        path = root / relative_path(name)
        checks[f"absent:{name}"] = not path.exists() and not path.is_symlink()
    after = hashes(root)
    for name in expected.get("unchanged", []):
        checks[f"unchanged:{name}"] = name in before and before[name] == after.get(name)
    if "index" in expected:
        result = subprocess.run(["git", "ls-files", "--stage", "-z"], cwd=root,
                                capture_output=True, check=False, timeout=10)
        staged = {}
        for entry in result.stdout.split(b"\0"):
            if not entry:
                continue
            info, name = entry.split(b"\t", 1)
            blob = subprocess.run(["git", "cat-file", "blob", info.split()[1].decode()],
                                  cwd=root, capture_output=True, check=False, timeout=10)
            staged[name.decode()] = blob.stdout.decode(errors="replace")
        checks["index"] = result.returncode == 0 and staged == {
            name: render(contents) for name, contents in expected["index"].items()
        }
    allowed_changes = set(expected.get("files", {})) | set(expected.get("absent", []))
    if "answer" in expected:
        allowed_changes.add("answer.json")
    changed = {
        name for name in set(before) | set(after)
        if before.get(name) != after.get(name)
    }
    checks["no_unexpected_changes"] = not (changed - allowed_changes)
    return {"verified_success": bool(checks) and all(checks.values()), "checks": checks}


def summarize_events(events):
    tools = {}
    usage = []
    errors = []
    completed = False
    for event in events:
        kind = event.get("type")
        item = event.get("item", {})
        if item.get("type") in TOOL_TYPES and item.get("id"):
            tools[item["id"]] = item
        if kind == "turn.completed":
            completed = True
            usage.append(event.get("usage") or {})
        if kind in {"error", "turn.failed"}:
            errors.append(event)
    fields = ("input_tokens", "cached_input_tokens", "output_tokens")
    normalized = {
        key: sum(row[key] for row in usage)
        if usage and all(isinstance(row.get(key), int) for row in usage) else None
        for key in fields
    }
    return {
        "host_tool_calls": len(tools),
        "shell_tool_calls": sum(v.get("type") == "command_execution" for v in tools.values()),
        "nonzero_shell_exits": sum(v.get("type") == "command_execution" and
                                   v.get("exit_code") not in (None, 0) for v in tools.values()),
        "failed_tool_items": sum(v.get("status") == "failed" for v in tools.values()),
        "agent_usage": {"reported_fields": usage, "normalized_counts": normalized,
                        "missing_fields": [key for key, value in normalized.items() if value is None]},
        "trace_errors": errors, "turn_completed": completed,
    }


def run_process(command, cwd, prompt, env, directory, seconds, max_tools, max_output_tokens):
    events = []
    reason = None
    started = time.monotonic()
    with (directory / "events.jsonl").open("xb") as trace, (directory / "stderr.txt").open("xb") as err:
        proc = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=err, start_new_session=True)
        proc.stdin.write(prompt.encode())
        proc.stdin.close()
        selector = selectors.DefaultSelector()
        selector.register(proc.stdout, selectors.EVENT_READ)
        pending = b""
        try:
            while selector.get_map():
                if time.monotonic() - started >= seconds:
                    reason = "wall_budget"
                    break
                for key, _ in selector.select(timeout=0.2):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    trace.write(chunk)
                    trace.flush()
                    pending += chunk
                    while b"\n" in pending:
                        line, pending = pending.split(b"\n", 1)
                        try:
                            event = json.loads(line)
                            if not isinstance(event, dict):
                                raise ValueError("event is not an object")
                            events.append(event)
                        except (ValueError, UnicodeError):
                            events.append({"type": "error", "error": "malformed_json_event"})
                    stats = summarize_events(events)
                    if stats["host_tool_calls"] > max_tools:
                        reason = "tool_budget"
                    output = stats["agent_usage"]["normalized_counts"]["output_tokens"]
                    if output is not None and output > max_output_tokens:
                        reason = "output_token_budget"
                if reason:
                    break
        finally:
            selector.close()
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            proc.stdout.close()
    if pending.strip():
        events.append({"type": "error", "error": "truncated_json_event"})
    stats = summarize_events(events)
    reason = reason or ("completed" if proc.returncode == 0 and stats["turn_completed"]
                        and not stats["trace_errors"] else "agent_failure")
    return {**stats, "terminal_reason": reason, "process_exit": proc.returncode,
            "duration_ms": round((time.monotonic() - started) * 1000)}


def codex_command(codex, root, arm, model, reasoning):
    filesystem = {":root": "deny", ":minimal": "read", str(root): "write",
                  str(Path(codex).resolve()): "read"}
    installed = shutil.which("grevi")
    if installed:
        for path in {str(Path(installed)), str(Path(installed).resolve())}:
            filesystem[path] = "deny"
    filesystem_toml = ", ".join(f"{json.dumps(key)}={json.dumps(value)}"
                                for key, value in filesystem.items())
    network_toml = ('enabled=true, domains={"classifier.dev"="allow"}'
                    if arm == "B" else "enabled=false")
    config = {
        "model_reasoning_effort": reasoning,
        "approval_policy": "never",
        "default_permissions": "experiment",
        "allow_login_shell": False,
        "project_doc_max_bytes": 0,
        "web_search": "disabled",
        "shell_environment_policy.inherit": "none",
    }
    command = [codex, "exec", "--ignore-user-config", "--ephemeral", "--json",
               "--skip-git-repo-check", "-m", model]
    command.extend(["-c", "permissions={experiment={filesystem={" + filesystem_toml +
                    "},network={" + network_toml + "}}}"])
    shell_env = {"PATH": str(root / ".grevi-bin") + ":/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                 "HOME": str(root), "XDG_CACHE_HOME": str(root / ".cache"),
                 "XDG_CONFIG_HOME": str(root / ".config"), "TMPDIR": str(root / ".tmp"),
                 "LANG": "en_US.UTF-8"}
    command.extend(["-c", "shell_environment_policy.set={" + ",".join(
        f"{key}={json.dumps(value)}" for key, value in shell_env.items()) + "}"])
    for key, value in config.items():
        command.extend(["-c", f"{key}={json.dumps(value)}"])
    for feature in ("memories", "plugins", "apps", "multi_agent", "shell_snapshot"):
        command.extend(["--disable", feature])
    for feature in ("skip_host_skill_discovery", "network_proxy"):
        command.extend(["--enable", feature])
    command.append("-")
    return command


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tasks", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New private run directory")
    parser.add_argument("--grevi", type=Path, required=True)
    parser.add_argument("--skill", type=Path, required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--reasoning", default="low")
    parser.add_argument("--seed", type=int, default=20260920)
    parser.add_argument("--repetitions", type=int, default=2)
    parser.add_argument("--seconds", type=float, default=1200)
    parser.add_argument("--max-tools", type=int, default=100)
    parser.add_argument("--max-output-tokens", type=int, default=100000)
    parser.add_argument("--execute", action="store_true", help="Run real agents; otherwise freeze only")
    args = parser.parse_args()
    if min(args.repetitions, args.seconds, args.max_tools, args.max_output_tokens) <= 0:
        parser.error("Budgets and repetitions must be positive")
    if args.execute:
        parser.error("Execution disabled: no verified OS boundary isolates episode reads and host skills")
    tasks_bytes = args.tasks.read_bytes()
    taskset = json.loads(tasks_bytes)
    if taskset.get("schema_version") != 1 or taskset.get("panel") != "synthetic_diagnostic":
        parser.error("Expected schema_version 1, panel synthetic_diagnostic")
    tasks = taskset["tasks"]
    validate_tasks(tasks)
    binary = args.grevi.read_bytes()
    skill = args.skill.read_text()
    codex = shutil.which("codex")
    if not codex:
        parser.error("Installed codex CLI required")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    os.chmod(output, 0o700)
    assignments = schedule(tasks, args.repetitions, args.seed)
    manifest = {
        "schema_version": 1, "claim_status": "exploratory_only", "limitations": LIMITATIONS,
        "fixture_manifest_hash": digest(tasks_bytes), "binary_sha256": digest(binary),
        "skill_sha256": digest(skill.encode()), "harness_sha256": digest(Path(__file__).read_bytes()),
        "codex_sha256": digest(Path(codex).read_bytes()),
        "codex_version": subprocess.run([codex, "--version"], capture_output=True, text=True,
                                         check=True, timeout=10).stdout.strip(),
        "platform": platform.platform(), "seed": args.seed, "assignment_schedule": assignments,
        "agent_config": {"model": args.model, "reasoning": args.reasoning},
        "budgets": {"seconds": args.seconds, "tools": args.max_tools,
                    "output_tokens": args.max_output_tokens},
        "prices": None, "cache_policy": "fresh episode; semantic cache isolation unverified",
    }
    write_json(output / "manifest.json", manifest)
    (output / "tasks.private.json").write_bytes(tasks_bytes)
    frozen = output / "grevi.frozen"
    frozen.write_bytes(binary)
    frozen.chmod(0o500)
    if not args.execute:
        print(json.dumps({"manifest": str(output / "manifest.json"), "executed": False}))
        return
    with (output / "episodes.jsonl").open("x") as ledger:
        for pair_id, assignment in enumerate(assignments):
            task = tasks[assignment["task_index"]]
            for arm in assignment["order"]:
                episode_id = f"{pair_id:04d}-{arm}"
                episode = output / episode_id
                root = episode / "workspace"
                materialize(task, root)
                prompt = COMMON_PROMPT + "\n"
                env = {key: value for key, value in os.environ.items()
                       if key in {"HOME", "USER", "PATH", "LANG", "TMPDIR", "CODEX_HOME"}}
                if arm == "B":
                    bindir = root / ".grevi-bin"
                    bindir.mkdir()
                    shutil.copyfile(frozen, bindir / "grevi")
                    (bindir / "grevi").chmod(0o500)
                    env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
                    (root / "GREVI_SKILL.md").write_text(skill)
                    prompt += "grevi is available. Read GREVI_SKILL.md. Use it when helpful; ordinary tools and fallback remain available."
                else:
                    prompt += "grevi is unavailable. Use the ordinary installed tools; do not use hosted semantic substitutes."
                before = hashes(root)
                command = codex_command(codex, root, arm, args.model, args.reasoning)
                row = {"episode_id": episode_id, "task_id": task["id"],
                       "family_id": task["family_id"], "endpoint": task["endpoint"],
                       "stratum": task["stratum"], "pair_id": pair_id,
                       "replicate": assignment["replicate"], "arm": arm,
                       "before_hashes": before, "start_unix": time.time(),
                       "jev_usage": {"tokens": None, "available": False},
                       "total_usd": None, "cost_available": False,
                       "grevi_calls": None, "http_attempts": None, "retries": None,
                       "semantic_errors": None, "fallback_count": None}
                write_json(episode / "assignment.json", row)
                try:
                    row.update(run_process(command, root, prompt, env, episode, args.seconds,
                                           args.max_tools, args.max_output_tokens))
                    result = grade(task, root, before)
                    row["verified_success"] = result["verified_success"] and row["terminal_reason"] == "completed"
                    write_json(episode / "grade.json", result)
                except (OSError, ValueError, subprocess.SubprocessError) as exc:
                    row.update(terminal_reason="harness_failure", verified_success=False,
                               harness_error=type(exc).__name__ + ": " + str(exc))
                row["after_hashes"] = hashes(root)
                write_json(episode / "episode.json", row)
                ledger.write(json.dumps(row, sort_keys=True) + "\n")
                ledger.flush()
                os.fsync(ledger.fileno())
                print(json.dumps({key: row[key] for key in
                                  ("episode_id", "task_id", "arm", "terminal_reason", "verified_success")}), flush=True)


if __name__ == "__main__":
    main()

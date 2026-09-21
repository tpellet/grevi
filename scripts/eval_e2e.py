# /// script
# requires-python = ">=3.11"
# ///
"""Live CLI acceptance matrix on fictional inputs plus one attributed public CI log.

Run with an explicit binary and a NEW output directory. The caller authorizes egress to
TypeSafe/classifier; only jevify reads credentials. Never deletes fixtures or old results.
This is operational acceptance/profiling, not a held-out agent A/B experiment.
"""

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import unittest
from collections import Counter, defaultdict
from pathlib import Path


INVOICE = "Invoice for office supplies. Total 42 dollars."
POEM = "A poem: roses are red, violets are blue."
FILENAMES = "notes.txt\ninvoice-2026-03.pdf\ncat.jpg\n"
REFUND = "Please refund my order. I do not want a replacement.\n"
LOG = "Compiling demo\nerror[E0425]: cannot find value `missing_name` in this scope\nerror: could not compile demo\n"
ENDPOINTS = ("pick", "why", "route", "filter", "is", "add", "sort", "capabilities", "robot-docs", "health", "init")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as file:
        json.dump(value, file, indent=2, sort_keys=True)
        file.write("\n")


def percentile(values, fraction):
    """Empirical nearest rank; small samples do not identify population tails."""
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)] if values else None


def resource_metrics(stderr):
    cpu = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    return {
        "cpu_user_seconds": float(cpu[2]) if cpu else None,
        "cpu_system_seconds": float(cpu[3]) if cpu else None,
        "max_rss_bytes": int(rss[1]) if rss else None,
    }


def sanitize(text, root, repository):
    for path, label in ((str(root), "<RUN>"), (str(repository), "<REPO>")):
        text = text.replace(path, label)
    # Retain request-ID presence, not support identifiers in publishable evidence.
    text = re.sub(r"req_[A-Za-z0-9_-]+", "<REQUEST_ID>", text)
    text = re.sub(r"/(?:private/)?var/folders/[^\s\"']+", "<SYSTEM_TEMP>", text)
    return text


def classify(exit_code, envelope, expected, checks, timed_out=False):
    if timed_out:
        return "timeout"
    required = {"ok", "command", "version", "exit_code", "data", "meta", "error"}
    if not isinstance(envelope, dict) or not required <= envelope.keys() or envelope["exit_code"] != exit_code:
        return "contract_failure"
    if exit_code in (4, 5) and exit_code != expected:
        return "unavailable"
    return "pass" if exit_code == expected and all(checks.values()) else "acceptance_failure"


class Matrix:
    def __init__(self, binary, root, repository, repeats, offline):
        root.mkdir(parents=True, exist_ok=False)
        self.root, self.repository = root.resolve(), repository
        self.binary = self.root / "jevify"
        shutil.copyfile(binary, self.binary)
        self.binary.chmod(0o700)
        self.repeats, self.offline = repeats, offline
        self.rows, self.unavailable, self.undo_logs = [], set(), {}
        self.inventory = self.root / "inventory.json"
        save(self.inventory, [
            {"name": "pwd", "summary": "print working directory"},
            {"name": "wc", "summary": "count lines, words, and bytes"},
            {"name": "false", "summary": "return an unsuccessful exit status without doing anything"},
        ])
        self.route_sentinel = self.root / "route-executed"
        self.route_bin = self.root / "bin"
        self.route_bin.mkdir()
        # A selected tool must never run, even when it would return a failure.
        for name in ("pwd", "false"):
            tool = self.route_bin / name
            tool.write_text('#!/bin/sh\nprintf executed > "$ROUTE_SENTINEL"\nexit 1\n')
            tool.chmod(0o700)
        self.public_log = (repository / "evals/why/cargo-01.log").read_text()
        self.public_gold = json.loads((repository / "evals/why/cargo-01.expect").read_text())
        save(self.root / "inputs.json", {
            "filenames": FILENAMES, "refund": REFUND, "fictional_log": LOG,
            "invoice": INVOICE, "poem": POEM, "public_log": self.public_log,
            "public_gold": self.public_gold,
        })

    def env(self, backend, cache=None, missing_key=False):
        env = {k: v for k, v in os.environ.items() if not k.startswith("JEVIFY_")}
        env.update(JEVIFY_BACKEND=backend, JEVIFY_BASE_URL={
            "typesafe": "https://api.typesafe.ai", "classifier": "https://classifier.dev"
        }[backend], JEVIFY_INVENTORY_FILE=str(self.inventory))
        env["PATH"] = str(self.route_bin) + os.pathsep + env.get("PATH", "")
        env["ROUTE_SENTINEL"] = str(self.route_sentinel)
        if cache is None:
            env["JEVIFY_NO_CACHE"] = "1"
        else:
            env["JEVIFY_CACHE_DIR"] = str(cache)
        if backend == "classifier" or missing_key:
            env.pop("TYPESAFE_API_KEY", None)
            env.pop("TYPESAFE_API_KEY_FILE", None)
        elif self.offline:
            env.pop("TYPESAFE_API_KEY_FILE", None)
            env["TYPESAFE_API_KEY"] = os.urandom(16).hex()
        return env

    def run(self, backend, name, args, stdin="", expected=0, check=None, cwd=None,
            network=True, cache=None, phase="matrix", missing_key=False):
        command = args[0] if args[0] in ENDPOINTS else args[-1]
        identity = f"{len(self.rows):04d}-{backend}-{name}"
        row = {"id": identity, "backend": backend, "case": name, "command": command,
               "phase": phase, "expected_exit": expected, "argv": args,
               "input_sha256": sha(stdin.encode()), "network_case": network}
        if network and (self.offline or backend in self.unavailable):
            row["status"] = "not_run_offline" if self.offline else "not_run_backend_unavailable"
            self.rows.append(row)
            save(self.root / f"{identity}.json", row)
            return row
        started = time.monotonic()
        wrapped = sys.platform == "darwin"
        argv = (["/usr/bin/time", "-l"] if wrapped else []) + [str(self.binary), "--json", *args]
        proc = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            encoding="utf-8",
            cwd=cwd or self.root,
            env=self.env(backend, cache, missing_key),
            start_new_session=True,
            shell=False,
        )
        timed_out = False
        try:
            stdout, stderr = proc.communicate(stdin, timeout=75)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(proc.pid, signal.SIGTERM)
            stdout, stderr = proc.communicate(timeout=10)
        row.update(exit_code=proc.returncode, wall_ms=round((time.monotonic() - started) * 1000, 3),
                   **resource_metrics(stderr))
        try:
            envelope = json.loads(stdout)
        except json.JSONDecodeError:
            envelope = None
        if isinstance(envelope, dict) and isinstance(envelope.get("data"), dict):
            if envelope["data"].get("undo_log"):
                self.undo_logs[identity] = envelope["data"]["undo_log"]
        checks = check(envelope) if check and isinstance(envelope, dict) else {}
        if isinstance(envelope, dict):
            checks["command_identity"] = envelope.get("command") == command
            if phase == "warm":
                meta = envelope.get("meta") or {}
                checks["warm_cache_hit"] = meta.get("cache_hits", 0) > 0
                checks["warm_no_inference"] = meta.get("requests") == 0
        row["checks"] = checks
        row["status"] = classify(proc.returncode, envelope, expected, checks, timed_out)
        row["stdout"] = sanitize(stdout, self.root, self.repository)
        row["stderr"] = sanitize(stderr, self.root, self.repository)
        row["argv"] = json.loads(sanitize(json.dumps(args), self.root, self.repository))
        row["envelope"] = json.loads(sanitize(json.dumps(envelope), self.root, self.repository))
        self.rows.append(row)
        save(self.root / f"{identity}.json", row)
        if network and row["status"] in ("unavailable", "timeout"):
            self.unavailable.add(backend)
        print(f"{identity}: {row['status']} exit={proc.returncode} wall_ms={row['wall_ms']}", flush=True)
        return row

    def fixture(self, name):
        path = self.root / name
        path.mkdir()
        return path

    def git_fixture(self, backend, token=False):
        path = self.fixture(f"git-{backend}-{'token' if token else 'color'}")
        subprocess.run(["git", "init", "-q", str(path)], check=True, timeout=15, shell=False)
        base = {"style.txt": "color=blue\n", "logging.txt": "level=info\n"}
        changed = {"style.txt": "color=green\n", "logging.txt": "level=debug\n"}
        if token:
            base["expiry.txt"], changed["expiry.txt"] = "token_expiry_seconds=3600\n", "token_expiry_seconds=7200\n"
        for name, content in base.items():
            (path / name).write_text(content)
        subprocess.run(["git", "-C", str(path), "add", *base], check=True, timeout=15, shell=False)
        for name, content in changed.items():
            (path / name).write_text(content)
        save(path / "expected-inputs.json", {"base": base, "changed": changed})
        return path

    def sort_fixture(self, name, folders=2):
        path = self.fixture(name)
        names = ["Finance", "Poetry"] if folders == 2 else ["Finance"] + [f"Folder{i:03d}" for i in range(1, folders)]
        for name in names:
            (path / name).mkdir()
        (path / "march.txt").write_text(INVOICE)
        if folders == 2:
            (path / "poem.txt").write_text(POEM)
        return path

    def backend(self, backend):
        data = lambda v: v.get("data") or {}
        match = lambda v: {"selected_original_invoice": (data(v).get("matches") or [{}])[0].get("text") == "invoice-2026-03.pdf"}
        why = lambda v: {"root_cause_line": (data(v).get("causes") or [{}])[0].get("line") == 2}
        picked_pwd = lambda v: {"tool_pwd": data(v).get("tool") == "pwd",
                                "sentinel_absent": not self.route_sentinel.exists(),
                                "route_fields": set(data(v)) == {"tool", "summary", "synopsis", "fit", "alternatives"}}
        filtered = lambda v: {"selected_original_invoice": [r.get("text") for r in data(v).get("records", [])] == ["invoice-2026-03.pdf"]}
        for command, args in [("capabilities", ["capabilities"]), ("robot-docs", ["robot-docs"]), ("init", ["init", "zsh"])]:
            self.run(backend, command, args, network=False)
        for topic in ["commands", "exit-codes", "examples", "privacy"]:
            self.run(backend, f"docs-{topic}", ["robot-docs", topic], network=False)
        self.run(backend, "init-bash", ["init", "bash"], network=False)
        self.run(backend, "docs-invalid", ["robot-docs", "invalid-topic"], expected=2, network=False)
        self.run(backend, "init-invalid", ["init", "invalid-shell"], expected=2, network=False)
        for command in ENDPOINTS:
            self.run(backend, f"invalid-{command}", ["--threshold", "2", command], expected=2, network=False)
        self.run(backend, "health", ["health"])
        self.run(backend, "pick-match", ["pick", "the invoice from March"], FILENAMES, check=match)
        self.run(backend, "pick-none", ["pick", "a railway timetable"], FILENAMES, expected=3)
        self.run(backend, "pick-empty", ["pick", "a line"], expected=6, network=False)
        self.run(backend, "pick-limit", ["pick", "a line"], "".join(f"item-{i}\n" for i in range(20001)), expected=6, network=False)
        self.run(backend, "filter-match", ["filter", "an invoice"], FILENAMES, check=filtered)
        self.run(backend, "why-cause", ["why"], LOG, check=why)
        self.run(backend, "why-none", ["why"], "Build completed successfully. All tests passed.\n", expected=3)
        self.run(backend, "why-empty", ["why"], expected=6, network=False)
        lo, hi = self.public_gold["lines"]
        self.run(backend, "why-public", ["why"], self.public_log,
                 check=lambda v: {"gold_span": lo <= (data(v).get("causes") or [{"line": -1}])[0].get("line", -1) <= hi})
        self.run(backend, "is-yes", ["is", "the customer requests a refund"], REFUND,
                 check=lambda v: {"yes": data(v).get("verdict") == "yes"})
        self.run(backend, "is-no", ["is", "the customer requests a refund"],
                 "Please send a replacement. I do not want a refund.\n", expected=1)
        self.run(backend, "is-empty", ["is", "contains text"], expected=6, network=False)
        self.run(backend, "is-band-invalid", ["is", "--band", "0.6", "contains text"], "text", expected=2, network=False)
        # Gold is whole-input uncertainty: the middle is deliberately outside the retained budget.
        self.run(backend, "is-truncated", ["is", "the document requests a refund"],
                 "Routine status. " * 7000 + "Please refund my order." + "Routine status. " * 7000, expected=3)
        self.run(backend, "route-found", ["route", "print the current working directory"], check=picked_pwd)
        self.run(backend, "route-none", ["route", "play a violin melody through speakers"], expected=3,
                 check=lambda v: {"no_tool": data(v).get("tool") is None, "sentinel_absent": not self.route_sentinel.exists()})
        self.run(backend, "route-no-child", ["route", "return an unsuccessful exit status without doing anything"],
                 check=lambda v: {"tool_false": data(v).get("tool") == "false", "sentinel_absent": not self.route_sentinel.exists()})
        self.run(backend, "route-detailed-request", ["route", "print the physical current working directory"], check=picked_pwd)
        self.run(backend, "route-reject-exec", ["route", "--exec", "x"], expected=2, network=False)  # Removed flag intentionally rejected.
        self.run(backend, "route-reject-yes", ["route", "--yes", "x"], expected=2, network=False)  # Removed flag intentionally rejected.
        self.run(backend, "route-reject-dry-run", ["route", "--dry-run", "x"], expected=2, network=False)  # Removed flag intentionally rejected.
        self.run(backend, "route-reject-no-args", ["route", "--no-args", "x"], expected=2, network=False)  # Removed flag intentionally rejected.
        git = self.git_fixture(backend)
        show = lambda name: subprocess.check_output(["git", "show", ":" + name], cwd=git, encoding="utf-8", timeout=15, shell=False)
        content = lambda: {"worktree_unchanged": (git / "style.txt").read_text() == "color=green\n" and (git / "logging.txt").read_text() == "level=debug\n"}
        self.run(backend, "add-dry", ["add", "--dry-run", "change the color from blue to green"], cwd=git,
                 check=lambda v: {**content(), "index_unchanged": show("style.txt") == "color=blue\n" and show("logging.txt") == "level=info\n"})
        self.run(backend, "add-none", ["add", "--dry-run", "implement video decoding"], cwd=git, expected=3)
        self.run(backend, "add-apply", ["add", "--yes", "change the color from blue to green"], cwd=git,
                 check=lambda v: {**content(), "exact_index": show("style.txt") == "color=green\n" and show("logging.txt") == "level=info\n"})
        token = self.git_fixture(backend, token=True)
        self.run(backend, "add-token-expiry", ["add", "--dry-run", "extend authentication token expiry"], cwd=token,
                 check=lambda v: {"expiry_selected": any(h.get("file") == "expiry.txt" and h.get("p", 0) >= 0.5 for h in data(v).get("hunks", []))})
        self.run(backend, "add-not-repository", ["add", "--dry-run", "a change"], expected=6, network=False)
        sort = self.sort_fixture(f"sort-{backend}")
        intact = lambda: (sort / "march.txt").exists() and (sort / "march.txt").read_text() == INVOICE and (sort / "poem.txt").exists() and (sort / "poem.txt").read_text() == POEM
        self.run(backend, "sort-dry", ["sort", "."], cwd=sort,
                 check=lambda v: {"two_moves": len(data(v).get("moves", [])) == 2, "source_bytes": intact()})
        applied = self.run(backend, "sort-apply", ["sort", ".", "--apply"], cwd=sort,
                           check=lambda v: {"destination_bytes": (sort / "Finance/march.txt").exists() and (sort / "Finance/march.txt").read_text() == INVOICE and (sort / "Poetry/poem.txt").exists() and (sort / "Poetry/poem.txt").read_text() == POEM})
        undo_log = self.undo_logs.get(applied["id"])
        if undo_log:
            self.run(backend, "sort-undo", ["sort", ".", "--undo", undo_log], cwd=sort, network=False,
                     check=lambda v: {"restored_bytes": intact()})
        else:
            self.rows.append({"backend": backend, "case": "sort-undo", "command": "sort", "phase": "matrix", "status": "not_run_no_usable_undo_log"})
        empty = self.fixture(f"empty-{backend}")
        self.run(backend, "sort-no-folders", ["sort", "."], cwd=empty, expected=6, network=False)
        for count in [99, 100]:
            boundary = self.sort_fixture(f"sort-{backend}-{count}", count)
            self.run(backend, f"sort-{count}-folders", ["sort", "."], cwd=boundary,
                     check=lambda v: {"correct_folder": any(str(m.get("to", "")).endswith("Finance/march.txt") for m in data(v).get("moves", []))})
        profiles = [
            ("pick", ["pick", "the invoice from March"], FILENAMES, None, match),
            ("why", ["why"], LOG, None, why),
            ("is", ["is", "the customer requests a refund"], REFUND, None, None),
            ("route", ["route", "print the current working directory"], "", None, picked_pwd),
            ("filter", ["filter", "an invoice"], FILENAMES, None, filtered),
            ("add", ["add", "--dry-run", "extend authentication token expiry"], "", token, None),
            ("sort", ["sort", "."], "", boundary, None),
        ]
        for command, args, stdin, cwd, check in profiles:
            for repeat in range(self.repeats):
                cache = self.root / f"cache-{backend}-{command}-{repeat}"
                self.run(backend, f"profile-{command}-{repeat}-cold", args, stdin, cwd=cwd, check=check, cache=cache, phase="cold")
                self.run(backend, f"profile-{command}-{repeat}-warm", args, stdin, cwd=cwd, check=check, cache=cache, phase="warm")
        for command, args in [("capabilities", ["capabilities"]), ("robot-docs", ["robot-docs"]),
                              ("init", ["init", "zsh"]), ("health", ["health"])]:
            for repeat in range(self.repeats):
                self.run(backend, f"profile-{command}-{repeat}", args, network=command == "health", phase="uncached")
        if backend == "typesafe":
            self.run(backend, "missing-auth", ["health"], expected=5, network=False, missing_key=True)

    def finish(self):
        groups = defaultdict(list)
        for row in self.rows:
            if "wall_ms" in row:
                groups[(row["backend"], row["command"], row["phase"], row["status"])].append(row)
        profiles = []
        for (backend, command, phase, status), rows in sorted(groups.items()):
            values = [r["wall_ms"] for r in rows]
            profiles.append({"backend": backend, "command": command, "phase": phase, "status": status,
                             "n": len(rows), "wall_ms": {"p50": percentile(values, .5), "p95": percentile(values, .95), "p99": percentile(values, .99)},
                             "max_rss_bytes": max((r["max_rss_bytes"] or 0 for r in rows), default=0) or None,
                             "cpu_seconds_sum": sum((r["cpu_user_seconds"] or 0) + (r["cpu_system_seconds"] or 0) for r in rows)})
        summary = {"status_counts": dict(Counter(r["status"] for r in self.rows)), "profiles": profiles,
                   "rows": self.rows, "limitations": ["Empirical percentiles only; n<100 does not resolve p99 population latency.",
                   "CPU/RSS are process counters, not allocation, CPU-stack or I/O profiles.",
                   "Outages and skipped cases are not semantic failures or passes.", "Synthetic/development inputs are not held-out accuracy evidence."]}
        save(self.root / "summary.json", summary)
        save(self.root / "manifest.json", {p.name: sha(p.read_bytes()) for p in sorted(self.root.iterdir()) if p.is_file()})
        print(json.dumps(summary["status_counts"], sort_keys=True), flush=True)


class HarnessTests(unittest.TestCase):
    def test_quantiles_keep_extremes_and_empty(self):
        self.assertIsNone(percentile([], .95))
        self.assertEqual(percentile([4, 1, 3, 2], .5), 2)
        self.assertEqual(percentile([4, 1, 3, 2], .99), 4)

    def test_failures_are_not_passes_or_semantic_misses(self):
        env = dict(ok=False, command="pick", version="0", exit_code=4, data=None, meta={}, error={})
        self.assertEqual(classify(4, env, 0, {}), "unavailable")
        self.assertEqual(classify(4, env, 4, {}), "pass")
        self.assertEqual(classify(0, env, 0, {}), "contract_failure")
        env["exit_code"] = 0
        self.assertEqual(classify(0, env, 0, {"gold": False}), "acceptance_failure")
        self.assertEqual(classify(0, env, 0, {}, True), "timeout")

    def test_resource_units_and_missing_values(self):
        self.assertEqual(resource_metrics("0.12 real 0.02 user 0.01 sys\n1234 maximum resident set size")["max_rss_bytes"], 1234)
        self.assertIsNone(resource_metrics("")["cpu_user_seconds"])

    def test_sanitization_retains_structure(self):
        text = json.dumps({"path": "/tmp/run/a", "request_id": "req_abc"})
        self.assertEqual(json.loads(sanitize(text, Path("/tmp/run"), Path("/repo"))), {"path": "<RUN>/a", "request_id": "<REQUEST_ID>"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--backends", nargs="+", choices=["typesafe", "classifier"], default=["typesafe", "classifier"])
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.main(argv=[sys.argv[0]], exit=False)
        if not result.result.wasSuccessful():
            raise SystemExit(1)
        return
    if not args.binary or not args.out or args.repeats < 1:
        parser.error("--binary, --out and --repeats >= 1 are required")
    matrix = Matrix(args.binary.resolve(), args.out, Path(__file__).resolve().parents[1], args.repeats, args.offline)
    for backend in args.backends:
        matrix.backend(backend)
    matrix.finish()


if __name__ == "__main__":
    main()

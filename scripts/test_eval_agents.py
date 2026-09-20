"""Harness contract tests, no hosted model calls and no fixture cleanup."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import eval_agents as harness


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="jevify-harness-test-"))
        self.task = {"id": "one", "family_id": "family-one", "endpoint": "pick",
                     "stratum": "small", "prompt": "Write answer.json",
                     "files": {"input.txt": "alpha\nbeta\n"},
                     "expected": {"answer": {"line": 2}, "unchanged": ["input.txt"]}}

    def test_balanced_schedule_and_reproducibility(self):
        tasks = [dict(self.task, id=f"pick-{i}") for i in range(5)] + [
            dict(self.task, id=f"why-{i}", endpoint="why", stratum=f"stratum-{i}")
            for i in range(5)
        ]
        plan = harness.schedule(tasks, 2, 17)
        self.assertEqual(plan, harness.schedule(tasks, 2, 17))
        self.assertEqual(len(plan), 20)
        self.assertEqual(len({(r["task_index"], r["replicate"]) for r in plan}), 20)
        for endpoint in ("pick", "why"):
            orders = [row["order"] for row in plan
                      if tasks[row["task_index"]]["endpoint"] == endpoint]
            self.assertEqual(orders.count("AB"), orders.count("BA"))

    def test_rejects_unsafe_and_duplicate_inputs(self):
        for name in ("../gold", "/tmp/gold", ".", "TASK.md", ".git/config"):
            with self.assertRaises(ValueError):
                harness.relative_path(name)
        with self.assertRaises(ValueError):
            harness.validate_tasks([])
        with self.assertRaises(ValueError):
            harness.validate_tasks([self.task, self.task])

    def test_segments_and_limits(self):
        self.assertEqual(harness.render({"segments": [{"text": "x\n", "repeat": 200}]}), "x\n" * 200)
        for repeat in (0, -1, True, 1000001):
            with self.assertRaises(ValueError):
                harness.render({"segments": [{"text": "x", "repeat": repeat}]})

    def test_artifact_grade_requires_answer_and_preservation(self):
        root = self.root / "task"
        harness.materialize(self.task, root)
        before = harness.hashes(root)
        self.assertFalse(harness.grade(self.task, root, before)["verified_success"])
        (root / "answer.json").write_text('{"line":2}')
        self.assertTrue(harness.grade(self.task, root, before)["verified_success"])
        (root / "input.txt").write_text("changed")
        self.assertFalse(harness.grade(self.task, root, before)["verified_success"])

    def test_artifact_grade_rejects_undeclared_side_effects(self):
        root = self.root / "side-effect"
        harness.materialize(self.task, root)
        before = harness.hashes(root)
        (root / "answer.json").write_text('{"line":2}')
        (root / "extra.txt").write_text("undeclared")
        result = harness.grade(self.task, root, before)
        self.assertFalse(result["verified_success"])
        self.assertFalse(result["checks"]["no_unexpected_changes"])

    def test_grade_uses_exact_bytes_and_json_types(self):
        file_task = dict(self.task, expected={"files": {"output.txt": "line\n"}})
        root = self.root / "exact"
        harness.materialize(file_task, root)
        before = harness.hashes(root)
        (root / "output.txt").write_bytes(b"line\r\n")
        self.assertFalse(harness.grade(file_task, root, before)["verified_success"])

        answer_task = dict(self.task, expected={"answer": {"failed": False, "cause": "none"}})
        answer_root = self.root / "typed-answer"
        harness.materialize(answer_task, answer_root)
        answer_before = harness.hashes(answer_root)
        (answer_root / "answer.json").write_text('{"failed":0,"cause":"none"}')
        self.assertFalse(harness.grade(answer_task, answer_root, answer_before)["verified_success"])

    def test_runtime_namespaces_are_ignored_but_other_artifacts_are_not(self):
        root = self.root / "runtime"
        harness.materialize(self.task, root)
        before = harness.hashes(root)
        (root / "answer.json").write_text('{"line":2}')
        for name in harness.RUNTIME_DIRS:
            directory = root / name
            directory.mkdir()
            (directory / "retained.txt").write_text("runtime artifact")
        self.assertTrue(harness.grade(self.task, root, before)["verified_success"])
        (root / "empty-extra").mkdir()
        self.assertFalse(harness.grade(self.task, root, before)["verified_success"])

    def test_symlink_answer_is_rejected(self):
        root = self.root / "task"
        harness.materialize(self.task, root)
        gold = self.root / "gold.json"
        gold.write_text('{"line":2}')
        (root / "answer.json").symlink_to(gold)
        self.assertFalse(harness.grade(self.task, root, harness.hashes(root))["verified_success"])

    def test_staged_and_worktree_outcomes_are_distinct(self):
        task = dict(self.task, init_git=True,
                    initial_files={"file.txt": "old\n"},
                    staged_files={"file.txt": "staged\n"},
                    files={"file.txt": "working\n"},
                    expected={"index": {"file.txt": "staged\n"},
                              "files": {"file.txt": "working\n"}})
        root = self.root / "git-task"
        harness.materialize(task, root)
        before = harness.hashes(root)
        head = harness.repository_head(root)
        self.assertTrue(harness.grade(task, root, before, head)["verified_success"])
        task["expected"]["index"]["file.txt"] = "working\n"
        self.assertFalse(harness.grade(task, root, before, head)["verified_success"])

    def test_index_grade_rejects_commit_conflict_and_executable_mode(self):
        task = dict(self.task, init_git=True, initial_files={"file.txt": "old\n"},
                    staged_files={"file.txt": "staged\n"}, files={},
                    expected={"index": {"file.txt": "staged\n"}})

        committed = self.root / "committed"
        harness.materialize(task, committed)
        before = harness.hashes(committed)
        head = harness.repository_head(committed)
        subprocess.run(["git", "-c", "user.name=Fixture", "-c",
                        "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                        "-c", "core.hooksPath=/dev/null", "commit", "-qm", "Forbidden"],
                       cwd=committed, check=True)
        self.assertFalse(harness.grade(task, committed, before, head)["verified_success"])

        executable = self.root / "executable"
        harness.materialize(task, executable)
        before = harness.hashes(executable)
        head = harness.repository_head(executable)
        subprocess.run(["git", "update-index", "--chmod=+x", "file.txt"],
                       cwd=executable, check=True)
        self.assertFalse(harness.grade(task, executable, before, head)["verified_success"])

        conflicted = self.root / "conflicted"
        harness.materialize(task, conflicted)
        before = harness.hashes(conflicted)
        head = harness.repository_head(conflicted)
        blob = subprocess.run(["git", "hash-object", "-w", "file.txt"], cwd=conflicted,
                              capture_output=True, text=True, check=True).stdout.strip()
        subprocess.run(["git", "update-index", "--index-info"], cwd=conflicted,
                       input=f"100644 {blob} 1\tfile.txt\n", text=True, check=True)
        self.assertFalse(harness.grade(task, conflicted, before, head)["verified_success"])

    def test_usage_unknown_is_not_zero_and_tools_deduplicate(self):
        events = [{"type": "item.started", "item": {"id": "a", "type": "command_execution"}},
                  {"type": "item.completed", "item": {"id": "a", "type": "command_execution",
                                                        "exit_code": 1, "status": "completed"}},
                  {"type": "turn.completed", "usage": {"input_tokens": 7, "output_tokens": 2}}]
        stats = harness.summarize_events(events)
        self.assertEqual(stats["host_tool_calls"], 1)
        self.assertEqual(stats["nonzero_shell_exits"], 1)
        self.assertEqual(stats["failed_tool_items"], 0)
        self.assertIsNone(stats["agent_usage"]["normalized_counts"]["cached_input_tokens"])
        self.assertIsNone(harness.summarize_events([])["agent_usage"]["normalized_counts"]["input_tokens"])

    def test_process_failure_and_timeout_keep_trace(self):
        failure = self.root / "failure"
        failure.mkdir()
        row = harness.run_process([sys.executable, "-c", "import sys; print('not json'); sys.exit(2)"],
                                  self.root, "", os.environ.copy(), failure, 2, 3, 20)
        self.assertEqual(row["terminal_reason"], "agent_failure")
        self.assertTrue((failure / "events.jsonl").read_bytes())
        timeout = self.root / "timeout"
        timeout.mkdir()
        row = harness.run_process([sys.executable, "-c", "import time; time.sleep(5)"],
                                  self.root, "", os.environ.copy(), timeout, 0.05, 3, 20)
        self.assertEqual(row["terminal_reason"], "wall_budget")

    def test_json_writer_never_replaces_attempts(self):
        target = self.root / "attempt.json"
        harness.write_json(target, {"value": 1})
        with self.assertRaises(FileExistsError):
            harness.write_json(target, {"value": 2})
        self.assertEqual(json.loads(target.read_text()), {"value": 1})


if __name__ == "__main__":
    unittest.main()

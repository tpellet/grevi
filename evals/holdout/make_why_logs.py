# /// script
# requires-python = ">=3.11"
# ///
"""Produce the `why` inputs of the source-held-out set: real failures, run here.

Every log in `evals/holdout/inputs/why/` is the combined stdout and stderr of a command this
script ran on a project it wrote itself in a temporary directory. No log comes from a public CI
run, so none of them can overlap `evals/why/corpus.jsonl`, whose lines are all GitHub Actions
output. The projects are a few files each; the two that pull a dependency pin it with `=`.

    python3 evals/holdout/make_why_logs.py            # write the logs and their meta files
    python3 evals/holdout/make_why_logs.py --list     # names only

Normalisation, so a log is the same bytes on another machine and holds no local path: the
temporary directory becomes `/tmp/holdout`, `$HOME` becomes `$HOME`, the cargo registry source
path becomes `$CARGO_HOME/registry`, elapsed times (`finished in 0.12s`, `Finished in ...`,
`took 3ms`) become `Xs`, and trailing whitespace goes. A log longer than 120 lines keeps its
last 120, the way a caller pipes a tail into `jevify why`.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

OUT = Path(__file__).resolve().parent / "inputs" / "why"
CLANG = "/usr/bin/clang"
MAX_LINES = 120

CARGO_TOML = """[package]
name = "probe"
version = "0.0.0"
edition = "2021"

[dependencies]
{deps}

[[bin]]
name = "probe"
path = "src/main.rs"
"""


def cargo(name, main, deps="", cmd=("build",), extra=None):
    files = {"Cargo.toml": CARGO_TOML.format(deps=deps), "src/main.rs": main}
    files.update(extra or {})
    return {"id": name, "files": files, "cmd": ["cargo", *cmd], "tool": "cargo"}


def py(name, files, cmd):
    return {"id": name, "files": files, "cmd": cmd, "tool": "python3"}


SCENARIOS = [
    cargo("rust-type-mismatch", """
fn width(label: &str) -> usize {
    label.chars().count()
}

fn pad(label: &str, target: usize) -> String {
    let gap = target - width(label);
    format!("{label}{}", " ".repeat(gap))
}

fn main() {
    let widest = ["alpha", "beta"].iter().map(|s| width(s)).max().unwrap();
    println!("{}", pad("alpha", widest));
    println!("{}", pad("beta", "12"));
}
"""),
    cargo("rust-unresolved-import", """
use std::collections::HashMap;
use std::collections::SortedMap;

fn main() {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    counts.insert("hit", 1);
    let ordered: SortedMap<&str, u32> = counts.into_iter().collect();
    println!("{ordered:?}");
}
"""),
    cargo("rust-borrow-conflict", """
fn main() {
    let mut names = vec![String::from("alpha"), String::from("beta")];
    let first = &names[0];
    names.push(String::from("gamma"));
    println!("the first entry is {first}");
}
"""),
    cargo("rust-test-assert", """
fn main() {
    println!("{}", normalise("  Two  Words  "));
}

pub fn normalise(raw: &str) -> String {
    raw.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(normalise("  Hello "), "hello");
    }

    #[test]
    fn collapses_inner_runs() {
        assert_eq!(normalise("  Two  Words  "), "two words");
    }
}
""", cmd=("test",)),
    cargo("rust-unwrap-none", """
use std::collections::HashMap;

fn settings() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("retries".to_string(), "3".to_string());
    map.insert("timeout_ms".to_string(), "500".to_string());
    map
}

fn main() {
    let config = settings();
    println!("retries: {}", config.get("retries").unwrap());
    println!("timeout: {}", config.get("timeout_ms").unwrap());
    println!("backoff: {}", config.get("backoff_ms").unwrap());
}
""", cmd=("run", "--quiet")),
    cargo("rust-index-panic", """
fn column(row: &str, n: usize) -> &str {
    let fields: Vec<&str> = row.split('\\t').collect();
    fields[n]
}

fn main() {
    let rows = ["id\\tname\\tstate", "1\\talpha\\topen", "2\\tbeta"];
    for row in rows {
        println!("{}", column(row, 2));
    }
}
""", cmd=("run", "--quiet")),
    cargo("rust-overflow-panic", """
fn shrink(total: u32, used: u32) -> u32 {
    total - used
}

fn main() {
    let quota = 100u32;
    for used in [10u32, 60, 120] {
        println!("left: {}", shrink(quota, used));
    }
}
""", cmd=("run", "--quiet")),
    cargo("rust-serde-derive-feature", """
use serde::Serialize;

#[derive(Serialize)]
struct Entry {
    name: String,
    hits: u32,
}

fn main() {
    let entry = Entry { name: "alpha".into(), hits: 3 };
    println!("{}", serde_json::to_string(&entry).unwrap());
}
""", deps='serde = "=1.0.228"\nserde_json = "=1.0.145"\n'),
    cargo("rust-wrong-arity", """
fn render(label: &str, width: usize) -> String {
    format!("{label:width$}")
}

fn main() {
    for label in ["alpha", "beta"] {
        println!("[{}]", render(label));
    }
}
"""),
    cargo("rust-missing-version", """
fn main() {
    println!("{}", tinyvec_probe::hello());
}
""", deps='tinyvec = "=999.0.0"\n'),
    py("py-keyerror", {"report.py": '''
import json
import sys

def load(path):
    with open(path) as handle:
        return json.load(handle)

def total(rows):
    return sum(row["amount"] for row in rows)

def main():
    data = load(sys.argv[1])
    print("rows:", len(data["entries"]))
    print("total:", total(data["entries"]))

main()
''', "data.json": json.dumps({"entries": [{"id": 1, "amount": 4}, {"id": 2, "value": 9}]}, indent=1)},
       ["python3", "report.py", "data.json"]),
    py("py-module-not-found", {"pipeline.py": '''
import csv
import statistics
import fastparquet

def read(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))

rows = read("counts.csv")
print("median:", statistics.median(int(r["n"]) for r in rows))
print("frames:", fastparquet.__version__)
''', "counts.csv": "n\\n3\\n7\\n11\\n"}, ["python3", "pipeline.py"]),
    py("py-attribute-on-none", {"lookup.py": '''
ROUTES = {"/health": "health", "/metrics": "metrics"}

def handler(path):
    return ROUTES.get(path)

def dispatch(paths):
    for path in paths:
        name = handler(path)
        print(path, "->", name.upper())

dispatch(["/health", "/metrics", "/status"])
'''}, ["python3", "lookup.py"]),
    py("py-json-decode", {"config.py": '''
import json

def read(path):
    with open(path) as handle:
        return json.load(handle)

conf = read("settings.json")
print("workers:", conf["workers"])
''', "settings.json": '{\n  "workers": 4,\n  "queue": "default",\n}\n'},
       ["python3", "config.py"]),
    py("py-recursion", {"tree.py": '''
def depth(node, seen=None):
    if not node["children"]:
        return 1
    return 1 + max(depth(child) for child in node["children"])

leaf = {"name": "leaf", "children": []}
branch = {"name": "branch", "children": [leaf]}
leaf["children"].append(branch)
print("depth:", depth(branch))
'''}, ["python3", "tree.py"]),
    py("py-unittest-failure", {"test_slug.py": '''
import unittest

def slug(title):
    return title.strip().lower().replace(" ", "-")

class SlugTests(unittest.TestCase):
    def test_simple(self):
        self.assertEqual(slug("Release Notes"), "release-notes")

    def test_punctuation(self):
        self.assertEqual(slug("Release, Notes!"), "release-notes")

    def test_trailing_space(self):
        self.assertEqual(slug("Release Notes "), "release-notes")

unittest.main(verbosity=2)
'''}, ["python3", "test_slug.py"]),
    {"id": "c-compile-error", "tool": "clang", "files": {"count.c": '''
#include <stdio.h>
#include <string.h>

static int words(const char *line) {
    int n = 0;
    for (const char *p = line; *p; p++) {
        if (*p == ' ') n++;
    }
    return n + 1;
}

int main(void) {
    const char *lines[] = {"one two", "three four five"};
    for (int i = 0; i < 2; i++) {
        printf("%d\\n", words(lines[i]));
    }
    printf("%d\\n", words(42));
    return 0;
}
'''}, "cmd": [CLANG, "-Wall", "-Werror", "-o", "count", "count.c"]},
    {"id": "c-link-undefined", "tool": "clang", "files": {"main.c": '''
#include <stdio.h>

int checksum(const char *data, int len);

int main(void) {
    const char *payload = "held-out";
    printf("checksum: %d\\n", checksum(payload, 8));
    return 0;
}
''', "util.c": '''
int doubled(int n) {
    return n * 2;
}
'''}, "cmd": [CLANG, "-o", "app", "main.c", "util.c"]},
    {"id": "make-compile-error", "tool": "make", "files": {"Makefile": '''CC = /usr/bin/clang
CFLAGS = -Wall

all: app

app: main.o parse.o
\t$(CC) $(CFLAGS) -o app main.o parse.o

main.o: main.c
\t$(CC) $(CFLAGS) -c main.c

parse.o: parse.c
\t$(CC) $(CFLAGS) -c parse.c

clean:
\trm -f app *.o
''', "main.c": '''
#include <stdio.h>
int parse(const char *s);
int main(void) { printf("%d\\n", parse("12")); return 0; }
''', "parse.c": '''
#include <stdlib.h>

int parse(const char *s) {
    return atoi(s, 10);
}
'''}, "cmd": ["make"]},
    {"id": "sh-set-e", "tool": "sh", "files": {"release.sh": '''#!/bin/sh
set -eu
echo "==> checking the working tree"
echo "warning: 2 files are untracked; they will not be packaged"
echo "==> reading the manifest"
echo "note: the manifest declares 3 assets"
echo "==> verifying the checksum file"
echo "  $(grep -c . checksums.txt) entries"
echo "==> verifying every asset"
while read -r sum name; do
  echo "  $name ($sum)"
  if [ ! -f "assets/$name" ]; then
    echo "release.sh: assets/$name is missing; the packaging step never wrote it" >&2
    exit 4
  fi
done < checksums.txt
echo "==> done"
''', "checksums.txt": "aaa alpha.tar.gz\\nbbb beta.tar.gz\\nccc gamma.tar.gz\\n",
        "assets/alpha.tar.gz": "x\\n", "assets/beta.tar.gz": "x\\n"},
     "cmd": ["sh", "release.sh"]},
    # Deliberate abstention material: a run that succeeds and says so, and a run whose only
    # complaints are warnings the tool itself calls non-fatal.
    cargo("rust-clean-build", """
fn main() {
    let mut total = 0u32;
    for n in 1..=10u32 {
        total += n;
    }
    println!("total: {total}");
}
""", cmd=("build", "--verbose")),
    {"id": "c-warnings-only", "tool": "clang", "files": {"warn.c": '''
#include <stdio.h>

int main(void) {
    int unused_total = 0;
    char buffer[8];
    sprintf(buffer, "%d", 4);
    printf("%s\\n", buffer);
    return 0;
}
'''}, "cmd": [CLANG, "-Wall", "-Wextra", "-o", "warn", "warn.c"]},
]


def normalise(text, workdir):
    text = text.replace(str(workdir), "/tmp/holdout")
    # The cargo home is found in the text, not in the environment, so a machine that moved it
    # normalises the same way.
    text = re.sub(r"\S*/\.cargo(?=/)", "$CARGO_HOME", text)
    text = text.replace(str(Path.home()), "$HOME")
    text = re.sub(r"(?i)(in|took)\s+\d+[.,]?\d*\s*(s|ms|seconds)\b", r"\1 Xs", text)
    text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)
    lines = [line.rstrip() for line in text.splitlines()]
    while lines and not lines[-1]:
        lines.pop()
    if len(lines) > MAX_LINES:
        lines = lines[-MAX_LINES:]
    return "\n".join(lines) + "\n"


def build(scenario):
    with tempfile.TemporaryDirectory(prefix="holdout-why-") as tmp:
        workdir = Path(tmp).resolve()
        for name, body in scenario["files"].items():
            path = workdir / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body.lstrip("\n").replace("\\n", "\n").replace("\\t", "\t")
                            if name.endswith((".csv", ".txt")) else body.lstrip("\n"))
        env = dict(os.environ, CARGO_TERM_COLOR="never", TERM="dumb", RUST_BACKTRACE="0",
                   PYTHONDONTWRITEBYTECODE="1", NO_COLOR="1")
        # One pipe for both streams, the way a caller writes `cargo test 2>&1 | jevify why`:
        # the lines keep the order the tool wrote them in.
        proc = subprocess.run(scenario["cmd"], cwd=workdir, env=env, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, timeout=900, check=False)
        return proc.returncode, normalise(proc.stdout.decode("utf-8", "replace"), workdir)


def tool_version(tool):
    binary = {"cargo": ["cargo", "--version"], "python3": ["python3", "--version"],
              "clang": [CLANG, "--version"], "make": ["make", "--version"],
              "sh": ["sh", "-c", "echo sh"]}[tool]
    out = subprocess.run(binary, capture_output=True, text=True, check=False, timeout=60).stdout
    return out.splitlines()[0].strip() if out.strip() else tool


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--only", action="append")
    args = parser.parse_args()
    if args.list:
        for scenario in SCENARIOS:
            print(scenario["id"])
        return 0
    if shutil.which("cargo") is None:
        sys.exit("cargo is needed to produce the rust logs")
    OUT.mkdir(parents=True, exist_ok=True)
    versions = {}
    for scenario in SCENARIOS:
        if args.only and scenario["id"] not in args.only:
            continue
        code, log = build(scenario)
        path = OUT / f"{scenario['id']}.log"
        path.write_text(log)
        tool = scenario["tool"]
        versions.setdefault(tool, tool_version(tool))
        (OUT / f"{scenario['id']}.meta.json").write_text(json.dumps({
            "id": scenario["id"],
            "produced_by": "evals/holdout/make_why_logs.py",
            "command": scenario["cmd"],
            "exit_code": code,
            "tool": versions[tool],
            "files": sorted(scenario["files"]),
            "lines": len(log.splitlines()),
            "sha256": hashlib.sha256(log.encode()).hexdigest(),
        }, indent=1) + "\n")
        print(f"{scenario['id']:<28} exit={code} lines={len(log.splitlines())}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

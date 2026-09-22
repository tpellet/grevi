# /// script
# requires-python = ">=3.11"
# ///
"""Point at the root cause of every case in `evals/why/corpus.jsonl` with `jevify why --json -n 3`; compare
hit@1 and hit@3 with two free regex baselines on the same files; print where found cases' `any` sits.

The manifest is the corpus: one line per case with `id`, `source` (a job URL or "hand-written"), `license`,
`lines` (the 1-based inclusive root-cause range) and `sha256` (of `<id>.log`). A `*.log` absent from the
manifest is skipped and named, never scored; a manifest entry without provenance, or whose log is missing
or has another hash, or whose content duplicates an earlier case, stops the run before any request is paid."""
import hashlib
import hmac
import json
import re
import statistics
import subprocess
import sys
from pathlib import Path

H = "./target/release/jevify"  # fixed: ubs's taint check rejects an argv-selected executable
WHY = Path("evals/why")
MANIFEST = WHY / "corpus.jsonl"
# Copied from src/cmd/why.rs (SIGNAL); the baselines use exactly what jevify's prefilter uses.
SIGNAL = re.compile(r"(?i)\b(error|err!|fail(ed|ure|s)?|fatal|panic(ked)?|exception|traceback|denied|not found|no such|cannot|can't|couldn't|undefined|unresolved|refused|timed? ?out|segmentation|abort(ed)?|killed|exit (code|status) [1-9]|assert)")


class ManifestError(Exception):
    """A corpus entry that cannot be scored as written: the run stops before it pays for anything."""


def load_corpus():
    cases, seen_ids, seen_hashes = [], set(), {}
    for ln, raw in enumerate(MANIFEST.read_text().splitlines(), 1):
        if not raw.strip():
            continue
        try:
            e = json.loads(raw)
        except json.JSONDecodeError as err:
            raise ManifestError(f"{MANIFEST}:{ln}: not JSON ({err})") from err
        cid = e.get("id")
        if not cid or cid in seen_ids:
            raise ManifestError(f"{MANIFEST}:{ln}: missing or duplicate id {cid!r}")
        for field in ("source", "license"):
            if not isinstance(e.get(field), str) or not e[field].strip():
                raise ManifestError(f"{MANIFEST}:{ln}: case {cid} lacks provenance field {field!r}")
        lo, hi = e.get("lines") or (None, None)
        if not (isinstance(lo, int) and isinstance(hi, int) and 1 <= lo <= hi):
            raise ManifestError(f"{MANIFEST}:{ln}: case {cid} has no valid root-cause range {e.get('lines')!r}")
        log = WHY / f"{cid}.log"
        if not log.is_file():
            raise ManifestError(f"{MANIFEST}:{ln}: case {cid} names a missing log {log}")
        digest = hashlib.sha256(log.read_bytes()).hexdigest()
        if not hmac.compare_digest(str(e.get("sha256")), digest):
            raise ManifestError(f"{MANIFEST}:{ln}: case {cid} content hash {digest} differs from the manifest")
        if digest in seen_hashes:
            raise ManifestError(f"{MANIFEST}:{ln}: case {cid} is byte-identical to case {seen_hashes[digest]}")
        seen_ids.add(cid); seen_hashes[digest] = cid
        cases.append((cid, log, lo, hi))
    if not cases:
        raise ManifestError(f"{MANIFEST}: no cases")
    return cases, seen_ids


try:
    cases, ids = load_corpus()
except ManifestError as e:
    print(f"ManifestError: {e}", file=sys.stderr); sys.exit(2)
skipped = sorted(p.name for p in WHY.glob("*.log") if p.stem not in ids)
for name in skipped:
    print(f"skipped (not in {MANIFEST.name}): {name}", file=sys.stderr)
hit = {"jevify": [0, 0], "first SIGNAL match": [0, 0], "last SIGNAL match": [0, 0]}  # name -> [hit@1, hit@3]
errors = 0; abstained = 0; anys = []; cost = 0.0; cost_unknown = 0; rows = []
Path("evals/out").mkdir(exist_ok=True)
def score(name, pointed, lo, hi):
    # `pointed` is ranked, 1-based; a hit is a pointed line inside the labelled root-cause range.
    hit[name][0] += bool(pointed) and lo <= pointed[0] <= hi
    hit[name][1] += any(lo <= p <= hi for p in pointed[:3])
for cid, log, lo, hi in cases:
    text = log.read_text()
    # Same line numbering as jevify (Rust `str::lines`): split on "\n", no empty line after a final newline.
    lines = text.split("\n")
    if lines and lines[-1] == "": lines.pop()
    sig = [i + 1 for i, l in enumerate(lines) if SIGNAL.search(l)]
    score("first SIGNAL match", sig, lo, hi); score("last SIGNAL match", sig[::-1], lo, hi)
    out = subprocess.run([H, "--json", "why", "-n", "3"], input=text, capture_output=True, text=True, check=False, timeout=600)
    # No envelope (a panic, a killed process) is an error row, never a crash: the run keeps what it paid for.
    try:
        v = json.loads(out.stdout)
    except json.JSONDecodeError:
        errors += 1; print(f"no envelope (exit {out.returncode}) for {log.name}: {out.stderr.strip()[:200]}", file=sys.stderr)
        rows.append({"case": cid, "pointed": [], "any": None, "expect": [lo, hi], "exit": out.returncode, "cost_usd": None}); continue
    d = v.get("data") or {}
    # Exit 3 is an honest "no failure found": a miss for the table, not an error.
    if not v["ok"] and v["exit_code"] != 3:
        errors += 1; print(f"error exit {v['exit_code']}: {v['error']['kind']} for {log.name}", file=sys.stderr)
    abstained += v["exit_code"] == 3
    pointed = [c["line"] for c in d.get("causes") or []]
    score("jevify", pointed, lo, hi)
    if "any" in d and d["any"] is not None: anys.append(d["any"])
    # A token count the backend left unknown makes cost_usd null: count it as unknown, never a crash.
    c = (v.get("meta") or {}).get("cost_usd")
    if isinstance(c, (int, float)): cost += c
    else: cost_unknown += 1
    rows.append({"case": cid, "pointed": pointed, "any": d.get("any"), "expect": [lo, hi], "exit": v["exit_code"], "cost_usd": c})
    Path("evals/out/why.json").write_text(json.dumps(rows, indent=1))
n = len(cases)
print(f"why: {n} cases  skipped {len(skipped)}  abstained {abstained}  errors {errors}  cost ${cost:.4f} ({cost_unknown} unknown)")
print(f"{'method':<20} {'hit@1':>8} {'hit@3':>8}")
for name, (h1, h3) in hit.items():
    print(f"{name:<20} {h1:>5}/{n:<3}{h3:>5}/{n:<3}")
if anys:
    # Every case holds a failure, so this is where found cases' absolute Noul sits against the 0.5 knob.
    print(f"data.any over {len(anys)} answered cases: min {min(anys):.2f}  median {statistics.median(anys):.2f}  (threshold {v['meta']['threshold']})")

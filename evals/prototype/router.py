"""Intent -> installed command router over this Mac's real inventory (1,121 commands).

Round 1: shard the inventory into Choice questions of <=200 options + NONE, plus a Noul
"does any listed command do this". Round 2: re-judge the union of each shard's top-3 with a
richer excerpt from each man page, Choice + NONE, plus one Noul per candidate for abstention.
Results are cached so re-runs are free.
"""
import hashlib, json, os, re, subprocess, sys, time, urllib.request
from concurrent.futures import ThreadPoolExecutor

S = os.path.dirname(os.path.abspath(__file__))
KEY = open(os.path.expanduser("~/.ssh/typesafe-ai-key")).read().strip()
CACHE_PATH = os.path.join(S, "router_cache.json")
CACHE = json.load(open(CACHE_PATH)) if os.path.exists(CACHE_PATH) else {}
TOKENS = {"n": 0}


def jev(state, qs):
    body = json.dumps({"model": "jev-latest", "state": state, "questions": qs}, sort_keys=True)
    h = hashlib.sha256(body.encode()).hexdigest()
    if h in CACHE:
        return CACHE[h]
    r = urllib.request.Request("https://api.typesafe.ai/v1/systemone", body.encode(),
                               {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
    for attempt in range(4):
        try:
            out = json.load(urllib.request.urlopen(r, timeout=120))
            break
        except urllib.error.HTTPError as e:
            if e.code in (429, 529) and attempt < 3:
                time.sleep(2 ** attempt)
                continue
            raise
    TOKENS["n"] += out["usage"]["input_tokens"]
    CACHE[h] = out
    return out


def save_cache():
    json.dump(CACHE, open(CACHE_PATH, "w"))


INV = json.load(open(os.path.join(S, "inventory.json")))
NAMES = sorted(INV)
SHARD = 200
SHARDS = [NAMES[i:i + SHARD] for i in range(0, len(NAMES), SHARD)]

_man_cache = {}


def man_excerpt(cmd, chars=500):
    """First lines of the DESCRIPTION section of the man page (or the one-liner)."""
    if cmd in _man_cache:
        return _man_cache[cmd]
    txt = ""
    try:
        p = subprocess.run(["man", cmd], capture_output=True, text=True, timeout=10,
                           env={**os.environ, "MANWIDTH": "200", "PAGER": "cat", "MANPAGER": "cat"})
        raw = re.sub(r".\x08", "", p.stdout)
        m = re.search(r"\nDESCRIPTION\n(.*?)(\n[A-Z][A-Z ]+\n|\Z)", raw, re.S)
        if m:
            txt = re.sub(r"\s+", " ", m.group(1)).strip()[:chars]
    except Exception:
        pass
    _man_cache[cmd] = f"{cmd}: {INV[cmd]}. {txt}" if txt else f"{cmd}: {INV[cmd]}"
    return _man_cache[cmd]


def round1(request):
    def one(shard):
        state = {"request": request, "commands": [f"{n}: {INV[n]}" for n in shard]}
        crit = {n: None for n in shard}
        crit["NONE"] = "none of the listed commands does what the request asks"
        qs = {
            "pick": {"type": "choice",
                     "instructions": "Which command in `commands` is the right tool to accomplish `request`? Choose NONE if no listed command does it.",
                     "criteria": crit},
            "any": {"type": "noul",
                    "instructions": "Is there a command in `commands` whose purpose is to accomplish `request`?",
                    "criteria": {"true": "at least one listed command is the right tool for the request",
                                 "false": "none of the listed commands is a fit for the request"}},
        }
        return jev(state, qs)["answers"]
    with ThreadPoolExecutor(len(SHARDS)) as ex:
        outs = list(ex.map(one, SHARDS))
    cands = []
    for a in outs:
        probs = a["pick"]["probabilities"]
        top = sorted(((p, n) for n, p in probs.items() if n != "NONE"), reverse=True)[:3]
        cands += [(p, n, a["any"]["noul"]) for p, n in top]
    cands.sort(reverse=True)
    return cands, max(a["any"]["noul"] for a in outs)


def round2(request, cands):
    names = [n for _, n, _ in cands]
    state = {"request": request, "commands": [man_excerpt(n) for n in names]}
    crit = {n: None for n in names}
    crit["NONE"] = "none of the listed commands does what the request asks"
    qs = {"pick": {"type": "choice",
                   "instructions": "Which command in `commands` is the right tool to accomplish `request`? Choose NONE if no listed command does it.",
                   "criteria": crit}}
    for i, n in enumerate(names):
        qs[f"fit_{n}"] = {"type": "noul",
                          "instructions": f"Would running the command described in `commands[{i}]` accomplish `request`?",
                          "criteria": {"true": "this command is a correct, direct way to accomplish the request",
                                       "false": "this command does something else, or only tangentially related"}}
    a = jev(state, qs)["answers"]
    fits = {n: a[f"fit_{n}"]["noul"] for n in names}
    return a["pick"], fits


def route(request, abstain=0.5):
    t = time.perf_counter()
    cands, any1 = round1(request)
    pick, fits = round2(request, cands[:12])
    dt = time.perf_counter() - t
    choice = pick["choice"]
    ranked = sorted(fits.items(), key=lambda kv: -kv[1])
    best, best_fit = ranked[0]
    decision = "abstain" if (choice == "NONE" or best_fit < abstain) else choice
    return {"request": request, "decision": decision, "choice": choice, "conf": pick["confidence"],
            "any": any1, "fits": ranked[:5], "latency": dt, "round1_top": [n for _, n, _ in cands[:5]]}


def bm25_top1(request):
    """Lexical baseline: BM25 over the one-line descriptions plus command names."""
    import math
    toks = lambda s: re.findall(r"[a-z0-9]+", s.lower())
    docs = {n: toks(n + " " + INV[n]) for n in NAMES}
    N = len(docs); avg = sum(map(len, docs.values())) / N
    df = {}
    for d in docs.values():
        for w in set(d): df[w] = df.get(w, 0) + 1
    q = toks(request); best = (0, None)
    for n, d in docs.items():
        s = 0
        for w in q:
            tf = d.count(w)
            if not tf: continue
            idf = math.log(1 + (N - df[w] + .5) / (df[w] + .5))
            s += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * len(d) / avg))
        if s > best[0]: best = (s, n)
    return best[1]


if __name__ == "__main__":
    reqs = [l.strip() for l in sys.stdin if l.strip()]
    for r in reqs:
        print(json.dumps(route(r)))
    save_cache()
    print(f"# tokens this run: {TOKENS['n']}  (${TOKENS['n'] * 0.042e-6:.4f})", file=sys.stderr)

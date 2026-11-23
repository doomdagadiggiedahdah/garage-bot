#!/usr/bin/env python3
"""Proof cache for the garagebot Kani crate.

Source regions in src/lib.rs are delimited by marker comments:

    // @prelude                      shared consts / types / spec fns
    // @end
    // @fn NAME                      contract attrs + signature ...
        // @body                     ... body below this line
    // @end
    // @harness NAME target=FN stubs=a,b uses=c,d
    // @end

Key(harness) = sha256( prelude
                     | full source of target fn
                     | CONTRACT hash of each stubbed callee (stub_verified)
                     | FULL source hash of each fn called un-stubbed (uses)
                     | harness source )

So a body edit to a callee does not disturb callers that stub it; a contract
edit does.  Passing keys are combined into a Merkle root in ROOT.txt.

    cache.py run [--only H] [--force]   run harnesses whose key changed
    cache.py status                     show what would re-run
    cache.py note KIND text...          append a note line to LOG.md
"""
import hashlib, json, re, subprocess, sys, time, os
from datetime import datetime, timezone

ROOT = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(ROOT, "src", "lib.rs")
CACHE = os.path.join(ROOT, ".proofcache.json")
LOG = os.path.join(ROOT, "LOG.md")
ROOTFILE = os.path.join(ROOT, "ROOT.txt")
TIMEOUT = int(os.environ.get("PROOF_TIMEOUT", "300"))  # seconds per harness


def h(*parts):
    m = hashlib.sha256()
    for p in parts:
        b = p.encode() if isinstance(p, str) else p
        m.update(len(b).to_bytes(8, "big"))
        m.update(b)
    return m.hexdigest()


def now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse():
    prelude, fns, harnesses = [], {}, {}
    cur = None
    for ln in open(SRC).read().splitlines():
        s = ln.strip()
        if s.startswith("// @prelude"):
            cur = ("prelude", None, [])
        elif s.startswith("// @fn "):
            cur = ("fn", s.split()[2], [])
        elif s.startswith("// @harness "):
            toks = s.split()[2:]
            meta = {"target": "", "stubs": [], "uses": []}
            for t in toks[1:]:
                k, v = t.split("=", 1)
                meta[k] = v if k == "target" else [x for x in v.split(",") if x]
            cur = ("harness", toks[0], [], meta)
        elif s == "// @end":
            if cur[0] == "prelude":
                prelude += cur[2]
            elif cur[0] == "fn":
                lines = cur[2]
                i = next((i for i, l in enumerate(lines) if l.strip() == "// @body"), None)
                if i is None:
                    sys.exit(f"fn {cur[1]}: missing // @body marker")
                fns[cur[1]] = {"contract": "\n".join(lines[:i]), "full": "\n".join(lines)}
            else:
                harnesses[cur[1]] = dict(cur[3], src="\n".join(cur[2]))
            cur = None
        elif cur is not None:
            cur[2].append(ln)
    return "\n".join(prelude), fns, harnesses


def strip_comments(t):
    return re.sub(r"//.*", "", t)


def lint(fns, harnesses):
    """Declared deps must match what the code actually calls."""
    errs = []
    for name, hz in harnesses.items():
        tgt = hz["target"]
        if tgt and tgt not in fns:
            errs.append(f"{name}: unknown target {tgt}")
            continue
        attr_stubs = set(re.findall(r"stub_verified\((\w+)\)", hz["src"]))
        if attr_stubs != set(hz["stubs"]):
            errs.append(f"{name}: stubs= {sorted(hz['stubs'])} but attributes say {sorted(attr_stubs)}")
        body = strip_comments(hz["src"])
        if tgt:
            body += strip_comments(fns[tgt]["full"])
        # transitive closure over un-stubbed uses
        seen, todo = set(), [body]
        while todo:
            b = todo.pop()
            for f in fns:
                if f in seen or f == tgt or f in hz["stubs"]:
                    continue
                if re.search(r"\b%s\s*\(" % re.escape(f), b):
                    seen.add(f)
                    todo.append(strip_comments(fns[f]["full"]))
        if seen != set(hz["uses"]):
            errs.append(f"{name}: uses= {sorted(hz['uses'])} but code calls un-stubbed {sorted(seen)}")
    if errs:
        sys.exit("dependency lint failed:\n  " + "\n  ".join(errs))


def keys():
    prelude, fns, harnesses = parse()
    lint(fns, harnesses)
    out = {}
    for name, hz in sorted(harnesses.items()):
        parts = ["prelude", h(prelude)]
        if hz["target"]:
            parts += ["target", h(fns[hz["target"]]["full"])]
        for s in sorted(hz["stubs"]):
            parts += ["stub:" + s, h(fns[s]["contract"])]
        for u in sorted(hz["uses"]):
            parts += ["use:" + u, h(fns[u]["full"])]
        parts += ["harness", h(hz["src"])]
        out[name] = h(*parts)
    return out


def merkle(leaves):
    if not leaves:
        return h("empty")
    level = leaves[:]
    while len(level) > 1:
        nxt = []
        for i in range(0, len(level), 2):
            nxt.append(h(level[i], level[i + 1]) if i + 1 < len(level) else level[i])
        level = nxt
    return level[0]


def load():
    return json.load(open(CACHE)) if os.path.exists(CACHE) else {}


def log(line):
    with open(LOG, "a") as f:
        f.write(line + "\n")


def run_kani(name):
    t0 = time.time()
    try:
        p = subprocess.run(
            ["timeout", "-k", "5", str(TIMEOUT), "cargo", "kani", "-Z", "function-contracts",
             "-Z", "stubbing", "--harness", name, "--exact"],
            cwd=ROOT, capture_output=True, text=True)
        out = p.stdout + p.stderr
        if p.returncode == 124:
            out += f"\nerror: TIMEOUT after {TIMEOUT}s"
            subprocess.run(["pkill", "-f", f"cbmc.*{os.path.basename(ROOT)}/target"])
    except Exception as e:  # noqa
        out = f"error: {e}"
    dt = time.time() - t0
    ok = "VERIFICATION:- SUCCESSFUL" in out and "VERIFICATION:- FAILED" not in out
    # a kani::cover! that cannot be reached means the harness is vacuous: count it as a failure
    covers = re.findall(r"\.cover\.\d+\s*\n\s*- Status: (\w+)", out)
    if any(c != "SATISFIED" for c in covers):
        ok = False
        out += "\nerror: unsatisfied cover (vacuous harness)"
    os.makedirs(os.path.join(ROOT, "target", "kani-logs"), exist_ok=True)
    open(os.path.join(ROOT, "target", "kani-logs", name + ".log"), "w").write(out)
    return ok, dt, out


def write_root(ks, cache):
    """Root over SOUND proofs: the harness passes, and every callee it stubs has a
    passing contract proof (target=callee) that is itself sound."""
    _, _, harnesses = parse()
    passed = {n for n, k in ks.items() if cache.get(k, {}).get("ok")}
    memo = {}

    def established(fn, seen=()):
        if fn not in memo:
            memo[fn] = fn not in seen and any(
                n in passed and all(established(s, seen + (fn,)) for s in hz["stubs"])
                for n, hz in harnesses.items() if hz["target"] == fn)
        return memo[fn]

    sound = sorted((n, ks[n]) for n in passed if all(established(s) for s in harnesses[n]["stubs"]))
    root = merkle([h(n, k) for n, k in sound])
    with open(ROOTFILE, "w") as f:
        f.write(f"root {root}\n")
        f.write(f"sound {len(sound)} / passing {len(passed)} / total {len(ks)} harnesses\n")
        f.write("leaf = sha256(harness name, key); pairwise sha256 up, odd node promoted\n")
        for n, k in sound:
            f.write(f"{k}  {n}\n")
    return root, len(sound)


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "status"
    if cmd == "note":
        log(f"- {now()} | NOTE | {sys.argv[2]} | {' '.join(sys.argv[3:])}")
        return
    ks = keys()
    cache = load()
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    force = "--force" in sys.argv
    stale = [n for n, k in ks.items() if force or k not in cache or not cache[k]["ok"]]
    if only:
        stale = [n for n in stale if n == only]
    if cmd == "status":
        for n, k in ks.items():
            print(f"{'RERUN ' if n in stale else 'cached'} {k[:12]} {n}")
        return
    ran, failed, secs = 0, [], 0.0
    for n in stale:
        ok, dt, out = run_kani(n)
        ran += 1
        secs += dt
        cache[ks[n]] = {"ok": ok, "secs": round(dt, 1), "harness": n, "at": now()}
        json.dump(cache, open(CACHE, "w"), indent=1)
        log(f"- {now()} | RUN | {n} | {'PASS' if ok else 'FAIL'} | {dt:.1f}s | key {ks[n][:12]}")
        print(f"{'PASS' if ok else 'FAIL'} {dt:6.1f}s {n}")
        if not ok:
            failed.append(n)
            tail = [l for l in out.splitlines() if "FAILURE" in l or "error" in l.lower() or "Failed Checks" in l]
            print("    " + "\n    ".join(tail[:25]))
    root, npass = write_root(ks, cache)
    cached = len(ks) - ran if not only else len(ks) - len(stale)
    log(f"- {now()} | SUMMARY | ran={ran} cached={len(ks)-ran} failed={len(failed)} wall={secs:.1f}s | passing={npass}/{len(ks)} | root {root[:16]}")
    print(f"ran={ran} cached={len(ks)-ran} failed={len(failed)} wall={secs:.1f}s root={root}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

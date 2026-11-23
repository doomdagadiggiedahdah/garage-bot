#!/usr/bin/env python3
"""Proof cache for the garagebot Kani crate.

Source regions in logic/src/{lib.rs,spec.rs,fns/*.rs,props.rs} are delimited by
marker comments:

    // @prelude                      shared consts / types / spec fns
    // @end
    // @fn NAME                      contract attrs + signature ...
        // @body                     ... body below this line
    // @end
    // @harness NAME target=FN stubs=a,b uses=c,d
    // @end

Key(harness) = sha256( prelude
                     | full source of target fn
                     | CONTRACT hash of each replaced callee (kani::stub)
                     | FULL source hash of each fn called un-stubbed (uses)
                     | harness source )

So a body edit to a callee does not disturb callers that stub it; a contract
edit does.  Passing keys are combined into a Merkle root in ROOT.txt.

    cache.py run [--only H] [--prefix P] [--force]   run harnesses whose key changed
                                        (--prefix prop_ = only the top-level properties)
    cache.py status                     show what would re-run
    cache.py note KIND text...          append a note line to LOG.md
"""
import hashlib, json, re, shutil, subprocess, sys, time, os
from datetime import datetime, timezone

ROOT = os.path.dirname(os.path.abspath(__file__))
LOGIC = os.path.join(ROOT, "logic")
SRCDIR = os.path.join(LOGIC, "src")


def sources():
    fns = os.path.join(SRCDIR, "fns")
    files = [os.path.join(SRCDIR, "lib.rs"), os.path.join(SRCDIR, "spec.rs")]
    files += sorted(os.path.join(fns, f) for f in os.listdir(fns) if f.endswith(".rs"))
    files.append(os.path.join(SRCDIR, "props.rs"))
    return "\n".join(open(f).read() for f in files)
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
    for ln in sources().splitlines():
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
        # a replaced logic function: #[kani::stub(f, ...)] (its spec_f twin is not a logic function)
        attr_stubs = set(x for x in re.findall(r"kani::stub\((\w+)\s*,", hz["src"]) if x in fns)
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
    # A logic function must be pure: the proofs replace it by its specification,
    # which is only right if it has no memory of earlier calls.
    for name, f in fns.items():
        m = re.search(r"\b(static|unsafe|Atomic\w*|Cell|RefCell|UnsafeCell|extern)\b", strip_comments(f["full"]))
        if m:
            errs.append(f"fn {name}: `{m.group(1)}` is not allowed in a logic function (it must be pure)")
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


def run_proofq(name):
    """Run one harness through the proofq queue on eE (fair share of the CPU with
    the other agents). proofq reports an unsatisfied kani::cover! as a failure."""
    t0 = time.time()
    who = os.environ.get("PROOFQ_WHO", "cache")
    cmd = ["proofq", "submit", ROOT, "--who", who, "--timeout", str(TIMEOUT), "--no-cex", "--harness", name]
    if os.environ.get("PROOFQ_PRIORITY"):
        cmd += ["--priority", os.environ["PROOFQ_PRIORITY"]]
    try:
        jid = subprocess.run(cmd, capture_output=True, text=True).stdout.strip().split()[-1]
        out = subprocess.run(["proofq", "wait", jid, "--json"], capture_output=True, text=True).stdout
        res = json.loads(out)
        hs = [x for x in res.get("harnesses", []) if x["name"] == name]
        ok = res.get("status") == "pass" and len(hs) == 1 and hs[0]["status"] == "pass"
        if not ok:
            out += f"\nerror: proofq job {jid} status {res.get('status')} stage {res.get('stage')}"
    except Exception as e:  # noqa
        ok, out = False, f"error: proofq: {e}"
    os.makedirs(os.path.join(LOGIC, "target", "kani-logs"), exist_ok=True)
    open(os.path.join(LOGIC, "target", "kani-logs", name + ".log"), "w").write(out)
    return ok, time.time() - t0, out


def run_kani(name):
    # default: through proofq when it is installed; GB_DIRECT=1 runs cargo kani here
    if shutil.which("proofq") and not os.environ.get("GB_DIRECT"):
        return run_proofq(name)
    t0 = time.time()
    try:
        p = subprocess.run(
            ["timeout", "-k", "5", str(TIMEOUT), "cargo", "kani", "-Z", "function-contracts",
             "-Z", "stubbing", "--harness", name, "--exact"],
            cwd=LOGIC, capture_output=True, text=True, env=dict(os.environ, GB_NO_VERIFIER="1"))
        out = p.stdout + p.stderr
        if p.returncode == 124:
            out += f"\nerror: TIMEOUT after {TIMEOUT}s"
            subprocess.run(["pkill", "-f", f"cbmc.*{os.path.basename(ROOT)}/logic/target"])
    except Exception as e:  # noqa
        out = f"error: {e}"
    dt = time.time() - t0
    ok = "VERIFICATION:- SUCCESSFUL" in out and "VERIFICATION:- FAILED" not in out
    # a kani::cover! that cannot be reached means the harness is vacuous: count it as a failure
    covers = re.findall(r"\.cover\.\d+\s*\n\s*- Status: (\w+)", out)
    if any(c != "SATISFIED" for c in covers):
        ok = False
        out += "\nerror: unsatisfied cover (vacuous harness)"
    os.makedirs(os.path.join(LOGIC, "target", "kani-logs"), exist_ok=True)
    open(os.path.join(LOGIC, "target", "kani-logs", name + ".log"), "w").write(out)
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
    cond = sorted((n, ks[n]) for n in passed)
    croot = merkle([h(n, k) for n, k in cond])
    with open(ROOTFILE, "w") as f:
        f.write(f"root {root}\n")
        f.write(f"conditional-root {croot}\n")
        f.write(f"sound {len(sound)} / passing {len(passed)} / total {len(ks)} harnesses\n")
        f.write("root: over SOUND proofs only (the harness passes and every function it stubs has a passing, sound contract proof)\n")
        f.write("conditional-root: over every PASSING proof; a proof listed with 'needs' holds only once those contract proofs pass\n")
        f.write("leaf = sha256(harness name, key); pairwise sha256 up, odd node promoted\n")
        soundset = {n for n, _ in sound}
        for n, k in cond:
            needs = [s for s in harnesses[n]["stubs"] if not established(s)]
            f.write(f"{k}  {n}{'' if n in soundset else '  needs contract proofs: ' + ','.join(needs)}\n")
    return root, len(sound), len(passed)


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
    prefix = sys.argv[sys.argv.index("--prefix") + 1] if "--prefix" in sys.argv else None
    if prefix:
        stale = [n for n in stale if n.startswith(prefix)]
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
    root, nsound, npass = write_root(ks, cache)
    # cached  = not re-run because a passing result for the same key exists
    # skipped = not run and not passing (filtered out by --only / --prefix)
    cached = sum(1 for n, k in ks.items() if n not in stale and cache.get(k, {}).get("ok"))
    skipped = len(ks) - ran - cached
    # passing = harnesses whose proof passes; sound = passing AND every contract it
    # stubs is itself proven (0 until the function bodies exist).
    summary = (f"ran={ran} cached={cached} skipped={skipped} failed={len(failed)} wall={secs:.1f}s | "
               f"passing={npass}/{len(ks)} sound={nsound}/{len(ks)} | root {root[:16]}")
    log(f"- {now()} | SUMMARY | {summary}")
    print(summary)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

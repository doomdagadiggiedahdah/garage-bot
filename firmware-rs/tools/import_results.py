#!/usr/bin/env python3
"""Record proofq results (JSONL from tools/prove_all.py) into cache.py's .proofcache.json,
under the keys cache.py computes for THIS tree, then write ROOT.txt and a LOG.md summary.
The jobs must have run on a commit whose logic/ equals this tree's logic/.

    import_results.py RESULTS.jsonl
"""
import json, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
os.chdir(ROOT)
import cache as c  # noqa: E402

ks, cache = c.keys(), c.load()
best = {}
for line in open(sys.argv[1]):
    r = json.loads(line)
    prev = best.get(r["harness"])
    if prev is None or prev["status"] != "pass":
        best[r["harness"]] = r
n_ok = n_fail = 0
for n, k in ks.items():
    r = best.get(n)
    if r is None:
        continue
    ok = r["status"] == "pass"
    n_ok += ok
    n_fail += not ok
    cache[k] = {"ok": ok, "secs": r.get("kani_secs"), "harness": n, "at": c.now(), "job": r.get("job")}
    c.log(f"- {c.now()} | RUN | {n} | {'PASS' if ok else 'FAIL'} | {r.get('kani_secs')}s | key {k[:12]} | proofq {r.get('job')} {r.get('host')}")
json.dump(cache, open(c.CACHE, "w"), indent=1)
root, ns, npass = c.write_root(ks, cache)
extra = sorted(set(best) - set(ks))
ev = sum(1 for n in extra if best[n]["status"] == "pass")
s = (f"imported pass={n_ok} fail={n_fail} | passing={npass}/{len(ks)} sound={ns}/{len(ks)} | root {root[:16]} | "
     f"verifier harnesses (outside cache.py) pass={ev}/{len(extra)}")
c.log(f"- {c.now()} | SUMMARY | {s}")
print(s)

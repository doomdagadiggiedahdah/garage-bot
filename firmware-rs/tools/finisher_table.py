#!/usr/bin/env python3
"""Print the finisher's per-body table for EXPERIMENT.md: source of each body, the hash of
its whitespace-normalised body region, whether an architect reference proof had the same
body + contract file + spec.rs + lib.rs, and this round's proof result.
    finisher_table.py FILL_REPORT.json RESULTS.jsonl REF_STRICT.json REF_CMP.json
"""
import hashlib, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
fill, res_p, strict_p, cmp_p = sys.argv[1:5]
fill = json.load(open(fill))
strict = json.load(open(strict_p))
cmpr = json.load(open(cmp_p))
res = {}
for l in open(res_p):
    r = json.loads(l)
    if r["harness"] not in res or r["status"] == "pass":
        res[r["harness"]] = r


def body_hash(fn):
    t = open(os.path.join(ROOT, "logic/src/fns", fn + ".rs")).read()
    b = re.search(r"// @body\n(.*?)\n// @end", t, re.S).group(1)
    return hashlib.sha256(re.sub(r"\s+", "", b).encode()).hexdigest()[:16]


print("| function | body source | body sha256[:16] (ws-normalised) | same as architect reference proof | contract proof (this round) |")
print("|---|---|---|---|---|")
for fn in sorted(fill):
    src = fill[fn]
    src = "fleet-1 Haiku `" + src.split(":", 1)[1] + "`" if src.startswith("haiku:") else "script (spec text)"
    if fn in strict:
        same = f"yes, whole file + spec.rs + lib.rs ({strict[fn][0]})"
    elif cmpr[fn]["ref"] and cmpr[fn]["ref"][2]:
        same = f"body only ({cmpr[fn]['ref'][0]}; spec.rs changed since)"
    else:
        same = "no"
    r = res.get("contract_" + fn, {})
    pr = f"{r.get('status', 'not run')} {r.get('kani_secs', '')}s {r.get('job', '')}".strip()
    print(f"| {fn} | {src} | `{body_hash(fn)}` | {same} | {pr} |")

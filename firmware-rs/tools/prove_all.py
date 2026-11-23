#!/usr/bin/env python3
"""Run many harnesses through proofq in parallel: one lane per git worktree
(proofq runs one job per worktree at a time). All worktrees must be at the same commit.

    prove_all.py OUT.jsonl --wt DIR [--wt DIR ...] [--timeout 900] H1 H2 ...
Each line of OUT.jsonl: {"harness", "status", "kani_secs", "job", "host", "note", "lane"}.
Harnesses already in OUT.jsonl with status pass are skipped (resume).
"""
import json, os, subprocess, sys, threading, queue, time

args = sys.argv[1:]
out = args.pop(0)
wts, timeout, hs = [], "900", []
while args:
    a = args.pop(0)
    if a == "--wt":
        wts.append(args.pop(0))
    elif a == "--timeout":
        timeout = args.pop(0)
    else:
        hs.append(a)
done = set()
if os.path.exists(out):
    for l in open(out):
        r = json.loads(l)
        if r["status"] == "pass":
            done.add(r["harness"])
q = queue.Queue()
for h in hs:
    if h not in done:
        q.put(h)
lock = threading.Lock()
PQ = os.path.expanduser("~/.local/bin/proofq")


def lane(i, wt):
    who = f"finisher-w{i}"
    while True:
        try:
            h = q.get_nowait()
        except queue.Empty:
            return
        t0 = time.time()
        p = subprocess.run([PQ, "submit", wt, "--who", who, "--timeout", timeout, "--no-cex", "--harness", h],
                           capture_output=True, text=True)
        jid = (p.stdout.strip().split() or ["?"])[-1]
        w = subprocess.run([PQ, "wait", jid, "--json"], capture_output=True, text=True)
        try:
            r = json.loads(w.stdout)
        except Exception:
            r = {"status": "error", "note": (p.stderr + w.stderr)[-500:]}
        hz = [x for x in r.get("harnesses", []) if x.get("name") == h]
        st = r.get("status")
        if st == "pass" and not (len(hz) == 1 and hz[0].get("status") == "pass"):
            st = "fail"
        rec = {"harness": h, "status": st, "kani_secs": r.get("kani_secs"), "wall": round(time.time() - t0, 1),
               "job": jid, "host": r.get("host"), "cached": r.get("cached"), "note": r.get("note"),
               "stage": r.get("stage"), "lane": i,
               "fail": [f.get("description") for x in hz for f in x.get("failures", [])][:5],
               "excerpt": (r.get("excerpt") or "")[-800:] if st != "pass" else ""}
        with lock:
            with open(out, "a") as f:
                f.write(json.dumps(rec) + "\n")
            print(f"{st:8s} {rec['kani_secs']}s {h} [{jid} {rec['host']}]", flush=True)


ts = [threading.Thread(target=lane, args=(i, w)) for i, w in enumerate(wts, 1)]
for t in ts:
    t.start()
for t in ts:
    t.join()
print("ALL DONE", flush=True)

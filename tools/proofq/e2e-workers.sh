#!/bin/bash
# proofq remote-worker test. Needs at least one enabled worker that is up (see README, "Remote workers").
#  1. submits N jobs at once (default 12) from N worktrees of a copy of ~/garagebot-kani-loop and
#     shows how they spread over the hosts, and that no host ran more than its slots;
#  2. submits N more, waits for one to run on a worker, then cuts that worker off (its ssh
#     connection is killed and workers.json is pointed at an unreachable address): the job must go
#     back to the queue and the worker must show DOWN; workers.json is then restored.
# Uses the live queue. Override with PQ=..., T=..., SRC=..., N=..., HARNESSES="a b c" and the PROOFQ_*
# variables. ONLY_SPREAD=1 stops after part 1.
set -u
PQ=${PQ:-$HOME/.local/bin/proofq}; N=${N:-12}; T=${T:-$HOME/proofq-test/e2e-workers}
SRC=${SRC:-$HOME/garagebot-kani-loop}; WJ=${PROOFQ_WORKERS:-$HOME/.config/proofq/workers.json}
OUT=$T/out; RUN=$(date +%s)
HARN=(${HARNESSES:-contract_poll_step prop_d_fresh_commands_dropped prop_d_poll_batch contract_uptime_accumulate_fixed})
rm -rf $T; mkdir -p $OUT
rsync -a --exclude target --exclude .git $SRC/ $T/repo/
cd $T/repo; echo target > .gitignore
git init -q . && git add -A && git -c user.name=e2e -c user.email=e2e@localhost commit -qm base
for i in $(seq -w 1 $N); do git worktree add -q --detach $T/w$i; done

batch() {  # $1 = tag; new content in every worktree, so nothing comes from the cache
  local pids=()
  for i in $(seq -w 1 $N); do
    echo "// e2e-workers $RUN $1 $i" >> $T/w$i/src/lib.rs
    $PQ submit $T/w$i --who e2e-$i --harness ${HARN[$((10#$i % ${#HARN[@]}))]} > $OUT/$1-$i.id 2>> $OUT/submit.err &
    pids+=($!)
  done; wait "${pids[@]}"
}
sample() {  # one line per second: per-host running counts as the daemon sees them, and the scopes on each worker
  while [ ! -f $OUT/stop ]; do
    $PQ status --json > $OUT/st.json 2>/dev/null && python3 - $OUT/st.json $WJ <<'PY' >> $OUT/samples
import json, subprocess, sys
d = json.load(open(sys.argv[1]))["daemon"]; row = []
try: opts = {w.get("name") or w["ssh"]: w.get("ssh_opts", []) for w in json.load(open(sys.argv[2]))["workers"]}
except Exception: opts = {}
for x in d["hosts"]:
    n = mem = ""
    if x["ssh"] and x["state"] == "up":
        r = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", *opts.get(x["name"], []), x["ssh"], "bash", "-s"], input=
            "export XDG_RUNTIME_DIR=/run/user/$(id -u); u=$(systemctl --user list-units 'proofq-job-*.scope' --state=running --no-legend --plain | awk '{print $1}');"
            " echo $(echo -n \"$u\" | grep -c .) $([ -n \"$u\" ] && systemctl --user show $(echo \"$u\" | head -1) -p MemoryMax --value)",
            capture_output=True, text=True).stdout.split()
        n, mem = (r + ["", ""])[:2]
    row.append(f"{x['name']}:{x['running']}:{x['slots']}:{x['state']}:{n}:{mem}")
print(" ".join(row))
PY
    sleep 1
  done
}
report() {  # $1 = tag
  for i in $(seq -w 1 $N); do $PQ wait $(cat $OUT/$1-$i.id) --json > $OUT/$1-$i.json; done
  python3 - $OUT $1 <<'PY'
import glob, json, sys
O, tag = sys.argv[1:]
rows = [json.load(open(f)) for f in sorted(glob.glob(f"{O}/{tag}-*.json"))]
slots, seen, scopes, mem = {}, {}, {}, {}
for ln in open(f"{O}/samples"):
    for cell in ln.split():
        name, run, sl, state, n, m = cell.split(":")
        slots[name] = max(slots.get(name, 0), int(sl)); seen[name] = max(seen.get(name, 0), int(run))
        if n: scopes[name] = max(scopes.get(name, 0), int(n))
        if m: mem.setdefault(name, set()).add(m)
ok = True
for r in rows:
    print(f"  {r['id']} {r['who']:8} {r['status']:6} host={r.get('host') or '-':14} kani={r.get('kani_secs')}s wall={r.get('seconds')}s"
          + (f" requeues={r['requeues']} ({r.get('requeue_note')})" if r.get("requeues") else ""))
    ok &= r["status"] == "pass"
for name in slots:
    mine = [r for r in rows if r.get("host") == name and r.get("started_at")]
    ev = sorted([(r["started_at"], 1) for r in mine] + [(r["finished_at"], -1) for r in mine])
    cur = mx = 0
    for _, d in ev: cur += d; mx = max(mx, cur)
    print(f"== {name}: {len(mine)} jobs, slots={slots[name]}, most running at once: daemon samples={seen[name]}, "
          f"job records={mx}" + (f", scopes seen on the worker={scopes.get(name, 0)}, their MemoryMax={sorted(mem.get(name, []))}" if name != "local" else ""))
    ok &= seen[name] <= slots[name] and mx <= slots[name] and scopes.get(name, 0) <= slots[name]
print(f"== {tag}:", "PASS" if ok else "FAIL", f"({sum(1 for r in rows if r.get('host') not in (None, 'local'))} of {len(rows)} jobs ran on remote workers)")
PY
}
sample & SP=$!
echo "== 1. $N submits at once"
batch spread; sleep 20; $PQ status | cut -c1-140; report spread

if [ -n "${ONLY_SPREAD:-}" ]; then touch $OUT/stop; wait $SP 2>/dev/null; exit; fi
echo "== 2. connection loss in the middle of a job"
batch loss
for _ in $(seq 120); do
  V=$($PQ status --json | python3 -c 'import json,sys
d = json.load(sys.stdin); ssh = {x["name"]: x["ssh"] for x in d["daemon"]["hosts"]}
for j in d["jobs"]:
    if j["state"] == "running" and j.get("host") not in (None, "local") and j["seconds"] > 5: print(j["id"], j["host"], ssh[j["host"]]); break')
  [ -n "$V" ] && break; sleep 1
done
if [ -z "$V" ]; then echo "no job reached a remote worker: FAIL"; else
  set -- $V; J=$1; H=$2; A=$3
  cp $WJ $OUT/workers.json.saved
  python3 - $WJ $H <<'PY'
import json, sys
p, h = sys.argv[1:]; d = json.load(open(p))
for w in d["workers"]:
    if (w.get("name") or w["ssh"]) == h: w["ssh"] = "proofq@192.0.2.1"     # TEST-NET-1: never answers
json.dump(d, open(p, "w"), indent=1)
PY
  pkill -f "$A bash -s" && echo "$J was running on $H: killed its ssh connection and pointed $H at 192.0.2.1"
  for _ in $(seq 60); do [ "$($PQ status $J --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["requeues"])')" != 0 ] && break; sleep 1; done
  sleep 12; $PQ status | cut -c1-140
  cp $OUT/workers.json.saved $WJ; echo "workers.json restored"
  report loss
  python3 - $OUT/loss-*.json <<PY
import json, sys
r = [x for x in map(lambda f: json.load(open(f)), sys.argv[1:]) if x["id"] == "$J"][0]
print("== requeue:", "PASS" if r["requeues"] >= 1 and r["status"] == "pass" else "FAIL",
      f"($J requeues={r['requeues']}, then {r['status']} on {r.get('host')}; note: {r.get('requeue_note')})")
PY
  sleep 40; $PQ status | sed -n 1,6p | cut -c1-140
fi
touch $OUT/stop; wait $SP 2>/dev/null

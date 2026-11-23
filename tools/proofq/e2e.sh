#!/bin/bash
# proofq end-to-end test. Builds git worktrees of a copy of ~/garagebot-kani-loop under
# ~/proofq-test/e2e, submits 10 jobs at once (incl. a duplicate, a timeout, a compile error,
# a failing proof), checks the concurrency cap, then the per-who limit and supersede rules.
# Stops and starts proofq.service for the queue-rule part. Takes about 4 minutes.
set -u
P=~/.local/bin/proofq; T=~/proofq-test/e2e; OUT=$T/out; RUN=$(date +%s)
J=~/.local/state/proofq/jobs
rm -rf $T; mkdir -p $T $OUT
rsync -a --exclude target ~/garagebot-kani-loop/ $T/repo/
cd $T/repo; echo target > .gitignore
git init -q . && git add -A && git -c user.name=e2e -c user.email=e2e@localhost commit -qm base
mk() { git -C $T/repo worktree add -q --detach $T/$1 2>/dev/null; echo "// e2e $RUN ${2:-$1}" >> $T/$1/src/lib.rs; }
for w in w1 w2 w3 w4 w5 w6 wslow wfail wbroken; do mk $w; done
mk wdup w1                                   # byte-identical to w1 -> same cache key
sed -i 's/t.wrapping_add(d) & TICKS_MASK$/t.wrapping_add(d) \& (TICKS_MASK >> 1)/' $T/wfail/src/lib.rs
sed -i 's/t.wrapping_add(d) & TICKS_MASK$/let oops: u8 = t; t.wrapping_add(d) \& TICKS_MASK/' $T/wbroken/src/lib.rs
cat >> $T/wslow/src/lib.rs <<'RS'
#[cfg(kani)]
#[kani::proof]
fn e2e_never_ends() { let mut i: u64 = kani::any(); while i != 0 { i = i.wrapping_mul(3).wrapping_add(1); } }
RS
sub() { n=$1; shift; $P submit "$@" > $OUT/$n.id 2> $OUT/$n.err; }
( while [ ! -f $OUT/stop ]; do
    u=$(systemctl --user list-units 'proofq-job-*.scope' --state=running --no-legend --plain | awk '{print $1}')
    n=$(echo -n "$u" | grep -c .); m=""
    [ -n "$u" ] && m=$(systemctl --user show $(echo "$u" | head -1) -p MemoryMax --value)
    echo "$n $(grep -l '"state": "ready"' $J/*.json 2>/dev/null | wc -l) $m"; sleep 0.4
  done > $OUT/samples ) &
echo "== 10 submits at once"
sub 01 $T/w1 --harness contract_poll_step &
sub 02 $T/w2 --harness prop_d_fresh_commands_dropped &
sub 03 $T/w3 --harness prop_d_poll_batch &
sub 04 $T/w4 --harness contract_poll_step --unwind 12 --solver cadical &
sub 05 $T/w5 --harness contract_uptime_accumulate_fixed &
sub 06 $T/w6 --harness prop_d_poll_batch --harness prop_c_uptime_drift --priority high --who architect &
sub 07 $T/wslow --harness e2e_never_ends --timeout 20 &
sub 08 $T/wfail --harness contract_ticks_add &
sub 09 $T/wbroken --harness contract_ticks_add &
sub 10 $T/wdup --harness contract_poll_step &
wait $(jobs -p | tail -n +2) 2>/dev/null
sleep 15; $P status
for n in 01 02 03 04 05 06 07 08 09 10; do id=$(cat $OUT/$n.id); $P wait $id --json > $OUT/$n.json; echo $? > $OUT/$n.rc; done
touch $OUT/stop; sleep 1
python3 - <<PY
import json,os
O="$OUT"; rows=[]
exp={"01":0,"02":0,"03":0,"04":0,"05":0,"06":0,"07":2,"08":1,"09":1,"10":0}
ok=True
for n in sorted(exp):
    r=json.load(open(f"{O}/{n}.json")); rc=int(open(f"{O}/{n}.rc").read()); rows.append(r)
    good = rc==exp[n]; ok&=good
    print(f"job {n} {r['id']} {r['who']:9} exit={rc} {'ok ' if good else 'BAD'} status={r['status']:8} stage={r.get('stage'):6} cached={r['cached']!s:5} kani={r.get('kani_secs')}s {r.get('note','')}")
ev=[]
for r in rows:
    if r["started_at"]: ev+=[(r["started_at"],1),(r["finished_at"],-1)]
cur=mx=0
for t,d in sorted(ev): cur+=d; mx=max(mx,cur)
S=[l.split() for l in open(f"{O}/samples") if l.strip()]
smax=max(int(s[0]) for s in S); waited=sum(1 for s in S if int(s[0])==6 and int(s[1])>0)
mem={s[2] for s in S if len(s)>2}
print(f"== concurrency: max running proofq-job scopes sampled={smax} ({len(S)} samples), max overlapping Kani intervals in job records={mx}, samples with 6 running AND jobs waiting for a slot={waited}")
print(f"== MemoryMax seen on job scopes: {mem} (4G = {4<<30})")
pair=sorted((rows[0],rows[9]),key=lambda r:r["cached"]); d=pair[1]   # w1 and wdup are identical; submit order is a race
dup_ok = d["cached"] and not pair[0]["cached"] and d.get("cached_from")==pair[0]["id"]
print(f"== duplicate (w1/wdup, identical content): {pair[0]['id']} ran Kani, {d['id']} cached={d['cached']} cached_from={d.get('cached_from')}")
f=json.load(open(f"{O}/08.json"))["harnesses"][0]; x=f["failures"][0]
print(f"== failing harness {f['name']}: {x['description'][:90]!r} at {x['file']}:{x['line']}: {x['source']!r}")
print("   counterexample:", [(v['index'],v['value']) for v in (f['counterexample'] or {}).get('values',[])])
c=json.load(open(f"{O}/09.json")); e=c["compile_errors"][0]; print(f"== compile error: {e['code']} {e['message']} at {e['file']}:{e['line']}; started_at={c['started_at']} (no Kani slot)")
ok &= smax<=6 and mx<=6 and waited>0 and dup_ok and mem=={str(4<<30)} and bool(f["counterexample"]) and c["started_at"] is None
print("== BATCH", "PASS" if ok else "FAIL")
PY
echo "== queue rules (daemon stopped so jobs stay queued)"
systemctl --user stop proofq
a=$($P submit $T/w1 --harness contract_ticks_diff --who lim 2>/dev/null)
b=$($P submit $T/w2 --harness contract_ticks_diff --who lim 2>/dev/null)
$P submit $T/w3 --harness contract_ticks_diff --who lim; echo "third job for --who lim: exit=$? (want 3, rejected)"
a2=$($P submit $T/w1 --harness contract_ticks_add --who lim); echo "resubmit from w1: $a -> $a2"
$P cancel $b
systemctl --user start proofq; sleep 2
$P wait $a | head -2; echo "old w1 job exit=${PIPESTATUS[0]} (want 2, superseded)"
$P wait $b | head -1; echo "cancelled job exit=${PIPESTATUS[0]} (want 2)"
$P wait $a2 | head -1; echo "new w1 job exit=${PIPESTATUS[0]} (want 0)"
$P log $a2 | tail -3

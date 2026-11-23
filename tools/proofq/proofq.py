#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""proofq - a small job queue for `cargo kani` runs on a shared box.

    proofq submit <worktree> [--harness NAME]... [--timeout S] [--who NAME] [--priority high]
                  [--unwind N] [--solver NAME] [-Z FEATURE]...
    proofq wait <id> [--json]        exit 0 pass, 1 fail, 2 error/timeout/cancelled
    proofq status [id] [--json]
    proofq log <id>
    proofq cancel <id>
    proofq daemon                    (run by proofq.service)

State is plain files: the CLI creates job files, the daemon is the only writer
after that. See ~/proofq/README.md.
"""
import argparse
import fcntl
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HOME = Path.home()
STATE = Path(os.environ.get("PROOFQ_STATE", HOME / ".local/state/proofq"))
CACHE = Path(os.environ.get("PROOFQ_CACHE", HOME / ".cache/proofq"))
CONFIG = Path(os.environ.get("PROOFQ_CONFIG", HOME / ".config/proofq/config.json"))
WORKERS = Path(os.environ.get("PROOFQ_WORKERS", CONFIG.parent / "workers.json"))   # remote hosts, read live
JOBS, LOGS, RESULTS, TARGETS = STATE / "jobs", STATE / "logs", STATE / "cache", CACHE / "target"
HEARTBEAT = STATE / "daemon.json"
HISTORY = STATE / "history.json"      # per harness: run time and peak memory, written by the daemon

DEFAULTS = {
    "workers": 6,                 # concurrent Kani jobs on this host (remote hosts: workers.json)
    "worker_memory_max": "3G",    # per Kani job on a remote worker, unless the worker sets memory_max
    "worker_recheck_secs": 30,    # how often an idle or down remote worker is probed
    "worker_min_free_gb": 10,     # a remote worker with less free disk is marked down
    "worker_check_timeout": 900,  # cargo check on a worker (its first one builds the dependencies)
    "precheck_workers": 2,        # concurrent cargo check/test pre-checks
    "memory_max": "4G",           # per Kani job (systemd scope MemoryMax)
    "precheck_memory_max": "3G",
    "default_timeout": 300,       # seconds of Kani time per job
    "max_timeout": 3600,
    "precheck": True,             # cargo check before a job takes a Kani slot
    "precheck_test": False,       # opt-in: also plain `cargo test` (a failing test then fails the job)
    "precheck_check_timeout": 180,
    "precheck_test_timeout": 60,
    "counterexample": True,       # on failure, re-run failing harnesses with concrete playback
    "counterexample_timeout": 300,  # CBMC takes ~100 s to build the trace of a contract harness
    "default_z": ["function-contracts", "stubbing"],
    "max_queued_per_who": 2,
    "share_deps": True,           # seed each new worktree target dir with already-built dependencies
    "seed_build_timeout": 900,
    "target_max_age_hours": 24,   # idle target dirs older than this are deleted
    "target_max_total_gb": 15,    # above this, the least recently used idle target dirs are deleted
    "min_free_gb": 20,            # no new jobs start while the (shared) disk has less free space
    "keep_jobs_days": 14,
    "keep_cache_days": 30,
    "route_small_peak_gb": 4.5,   # learned routing: harnesses that peaked below this go to the remote workers
    "big_memory_max_gb": 8,       # a local job that peaked above memory_max before gets peak*1.25 (up to this)
}
WAITING = ("queued", "checking", "ready")
EXIT = {"pass": 0, "fail": 1}


def cfg():
    c = dict(DEFAULTS)
    try:
        c.update(json.loads(CONFIG.read_text()))
    except FileNotFoundError:
        pass
    except Exception as e:  # a broken config must not stop the queue
        print(f"proofq: ignoring bad config {CONFIG}: {e}", file=sys.stderr)
    return c


def tool_env():
    env = dict(os.environ)
    env["PATH"] = f"{HOME}/.cargo/bin:{HOME}/.local/bin:" + env.get("PATH", "/usr/bin:/bin")
    return env


def now():
    return time.time()


def write_json(path, obj):
    tmp = path.with_name(path.name + f".tmp{os.getpid()}.{threading.get_ident()}")
    tmp.write_text(json.dumps(obj, indent=1))
    os.replace(tmp, path)


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def ensure_dirs():
    for d in (JOBS, LOGS, RESULTS, TARGETS):
        d.mkdir(parents=True, exist_ok=True)


def all_jobs():
    out = []
    for f in JOBS.glob("*.json"):
        j = read_json(f)
        if j:
            out.append(j)
    return sorted(out, key=lambda j: j["seq"])


_hist = (None, {})


def history():
    """{harness name: {local_secs, remote_secs, peak_mb, needs_local, runs, at}}, re-read when the file changes."""
    global _hist
    try:
        st = HISTORY.stat()
        stamp = (st.st_mtime_ns, st.st_size)
    except OSError:
        return {}
    if stamp != _hist[0]:
        _hist = (stamp, (read_json(HISTORY) or {}).get("harnesses", {}))
    return _hist[1]


UNKNOWN_SECS = 1e5      # a harness never run here: schedule it early, so that its time and memory get known


def est_secs(j):
    """Expected Kani seconds of a job on this host, from history (unknown harness: UNKNOWN_SECS)."""
    hist, tot = history(), 0.0
    if not j.get("harnesses"):
        return UNKNOWN_SECS
    for n in j["harnesses"]:
        e = hist.get(n)
        if not e:
            return UNKNOWN_SECS
        tot += e.get("local_secs") or (e.get("remote_secs") or 0) * 0.4
    return tot


def mem_mb(s):
    m = re.fullmatch(r"\s*([\d.]+)\s*([KMGT]?)i?B?\s*", str(s), re.I)
    if not m:
        return 4096.0
    return float(m.group(1)) * {"": 1 / 1048576, "K": 1 / 1024, "M": 1, "G": 1024, "T": 1048576}[m.group(2).upper()]


def route_class(j, c):
    """small / big / unknown from the job's history: big = a harness peaked at route_small_peak_gb or
    more, or could not finish on a worker; small = all harnesses known and below that."""
    if j.get("local_only"):
        return "big"
    hist, peak = history(), 0.0
    if not j.get("harnesses"):
        return "unknown"
    for n in j["harnesses"]:
        e = hist.get(n)
        if not e or e.get("peak_mb") is None:
            return "unknown"
        if e.get("needs_local"):
            return "big"
        peak = max(peak, e["peak_mb"])
    return "big" if peak >= c["route_small_peak_gb"] * 1024 else "small"


def job_peak_mb(j):
    hist = history()
    return max([(hist.get(n) or {}).get("peak_mb") or 0 for n in j.get("harnesses", [])] or [0])


def order_key(j):
    """high priority first, then the longest expected run first, then submission order."""
    return (0 if j.get("priority") == "high" else 1, -est_secs(j), j["seq"])


# ---------------------------------------------------------------- hashing

def h(*parts):
    """Length-prefixed sha256, the same scheme as garagebot-kani-loop/cache.py."""
    m = hashlib.sha256()
    for p in parts:
        b = p.encode() if isinstance(p, str) else p
        m.update(len(b).to_bytes(8, "big"))
        m.update(b)
    return m.hexdigest()


def toplevel(crate):
    r = subprocess.run(["git", "-C", str(crate), "rev-parse", "--show-toplevel"], capture_output=True)
    if r.returncode == 0 and r.stdout.strip():
        return Path(os.fsdecode(r.stdout.strip())).resolve(), True
    return crate, False


def source_files(crate):
    """(top, sorted relative paths). Git: tracked + untracked-not-ignored files of the
    whole worktree, plus Cargo.lock even if ignored. No git: walk, skipping target/.git."""
    top, is_git = toplevel(crate)
    if is_git:
        out = subprocess.run(["git", "-C", str(top), "ls-files", "-co", "--exclude-standard", "-z"],
                             capture_output=True).stdout
        files = {os.fsdecode(p) for p in out.split(b"\0") if p}
        for lock in (top / "Cargo.lock", crate / "Cargo.lock"):
            if lock.exists():
                files.add(os.path.relpath(lock, top))
    else:
        files = set()
        for d, dirs, names in os.walk(top):
            dirs[:] = [x for x in dirs if x not in ("target", ".git")]
            files.update(os.path.relpath(os.path.join(d, n), top) for n in names)
    return top, sorted(files)


def tree_hash(crate):
    top, files = source_files(crate)
    parts = []
    for f in files:
        p = top / f
        try:
            if p.is_symlink():
                data = b"L" + os.fsencode(os.readlink(p))
            elif p.is_file():
                data = p.read_bytes()
            else:
                data = b"<absent>"
        except OSError:
            data = b"<unreadable>"
        parts += [f, hashlib.sha256(data).hexdigest()]
    return h(*parts)


_kani_ver = None


def kani_version():
    global _kani_ver
    if _kani_ver is None:
        try:
            r = subprocess.run(["cargo", "kani", "--version"], capture_output=True, text=True,
                               env=tool_env(), timeout=30)
            _kani_ver = (r.stdout.strip().splitlines() or ["unknown"])[0]
        except Exception:
            _kani_ver = "unknown"
    return _kani_ver


def kani_flags(job):
    a = []
    for z in job.get("z", []):
        a += ["-Z", z]
    if job.get("unwind") is not None:
        a += ["--unwind", str(job["unwind"])]
    if job.get("default_unwind") is not None:
        a += ["--default-unwind", str(job["default_unwind"])]
    if job.get("solver"):
        a += ["--solver", job["solver"]]
    a += job.get("kani_args", [])
    return a


# ---------------------------------------------------------------- per-harness keys
# Same idea as garagebot-*/cache.py (marker regions `// @prelude`, `// @fn NAME` ... `// @body`,
# `// @harness NAME target=F stubs=a,b uses=c`, closed by `// @end`), but self-contained and
# stricter: a harness's key covers the prelude, the full text of its target and of every
# region function whose name appears (as a word) in what it runs, the CONTRACT of every
# function it stubs, its own source, all of lib.rs and spec.rs, the lines of every other .rs
# file that lie outside any region, every non-.rs build input (Cargo.toml/.lock, build.rs,
# toolchain, .cargo, ...), the Kani version and the flags. Anything that cannot be parsed
# safely falls back to the whole-worktree key (tree_hash).

MARK_RX = re.compile(r"^\s*//\s*@(prelude|fn|body|harness|end)\b(.*)$")
WORD_RX = re.compile(r"\w+")
NOKEY_SUFFIX = (".md",)                      # documentation never reaches the compiler
NOKEY_NAMES = {".proofcache.json", "LOG.md", "ROOT.txt"}   # generated by cache.py runs
WHOLE_RS = ("lib.rs", "spec.rs")


def _parse_regions(files):
    """files: {relpath: text}. -> (prelude, fns, harnesses, outside) or None if not parseable.
    outside[relpath] = lines of that file not inside any region and not marker lines."""
    prelude, fns, harnesses, outside = [], {}, {}, {}
    for rel, text in sorted(files.items()):
        cur, out = None, []
        for ln in text.splitlines():
            m = MARK_RX.match(ln)
            if not m:
                (cur[2] if cur else out).append(ln)
                continue
            kind, rest = m.groups()
            if kind in ("prelude", "fn", "harness"):
                if cur:
                    return None                     # unterminated region
                if kind == "prelude":
                    cur = ("prelude", None, [])
                elif kind == "fn":
                    if not rest.split():
                        return None
                    cur = ("fn", rest.split()[0], [])
                else:
                    toks = rest.split()
                    if not toks:
                        return None
                    meta = {"target": "", "stubs": [], "uses": []}
                    for t in toks[1:]:
                        if "=" not in t:
                            return None
                        k, v = t.split("=", 1)
                        meta[k] = v if k == "target" else [x for x in v.split(",") if x]
                    cur = ("harness", toks[0], [], meta)
            elif kind == "body":
                if not cur or cur[0] != "fn":
                    return None
                cur[2].append(ln)                    # kept in the text; the split is by the marker line
            else:                                    # end
                if not cur:
                    return None
                if cur[0] == "prelude":
                    prelude += cur[2]
                elif cur[0] == "fn":
                    lines = cur[2]
                    i = next((i for i, l in enumerate(lines) if MARK_RX.match(l) and MARK_RX.match(l).group(1) == "body"), None)
                    if i is None or cur[1] in fns:
                        return None
                    fns[cur[1]] = {"contract": "\n".join(lines[:i]), "full": "\n".join(lines)}
                else:
                    if cur[1] in harnesses:
                        return None
                    harnesses[cur[1]] = dict(cur[3], src="\n".join(cur[2]))
                cur = None
        if cur:
            return None
        outside[rel] = "\n".join(out)
    return "\n".join(prelude), fns, harnesses, outside


def _frame_files(crate, top):
    """Relative-to-top paths of every file that may feed the build of `crate`, or None to fall
    back to the whole worktree (workspace, path dependencies outside the crate, include! out of it)."""
    _, files = source_files(crate)
    rel_crate = os.path.relpath(crate, top)
    keep = []
    for f in files:
        base = os.path.basename(f)
        if base in NOKEY_NAMES or f.endswith(NOKEY_SUFFIX):
            continue
        inside = rel_crate == "." or f == rel_crate or f.startswith(rel_crate + os.sep)
        # top-level build inputs outside the crate dir that cargo may read: manifests, lock, toolchain, .cargo
        top_cfg = os.path.dirname(f) in ("", ".cargo") and \
            base in ("Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml", "config", "config.toml")
        if inside or top_cfg:
            keep.append(f)
    for f in keep:
        p = top / f
        if base_is(f, "Cargo.toml"):
            try:
                t = p.read_text(errors="replace")
            except OSError:
                return None
            if re.search(r"^\s*\[workspace\b", t, re.M):
                return None
            for m in re.finditer(r"\bpath\s*=\s*\"([^\"]+)\"", t):
                tgt = (p.parent / m.group(1)).resolve()
                if not str(tgt).startswith(str(crate.resolve()) + os.sep) and tgt != crate.resolve():
                    return None                      # path dependency outside the crate
        elif f.endswith(".rs"):
            try:
                t = p.read_text(errors="replace")
            except OSError:
                return None
            for m in re.finditer(r"\binclude(?:_str|_bytes)?!\s*\(\s*\"([^\"]*)\"", t):
                if m.group(1).startswith("/") or ".." in m.group(1):
                    return None
    return keep


def base_is(f, name):
    return os.path.basename(f) == name


def _read(p):
    try:
        if p.is_symlink():
            return b"L" + os.fsencode(os.readlink(p))
        return p.read_bytes() if p.is_file() else b"<absent>"
    except OSError:
        return b"<unreadable>"


def harness_keys(crate, harnesses, exact=True):
    """{harness name: key} for the named harnesses, or None if the per-harness scheme cannot be
    used (then the caller uses the whole-worktree key)."""
    if not harnesses or not exact:
        return None
    crate = Path(crate)
    top, is_git = toplevel(crate)
    if not is_git:
        return None
    keep = _frame_files(crate, top)
    if keep is None:
        return None
    rs_text, other = {}, []
    for f in keep:
        if f.endswith(".rs"):
            rs_text[f] = _read(top / f).decode(errors="replace")
        else:
            other += [f, hashlib.sha256(_read(top / f)).hexdigest()]
    # lib.rs and spec.rs of the crate go in whole into every key, and are not parsed for regions
    whole = {f: t for f, t in rs_text.items() if os.path.basename(f) in WHOLE_RS and os.path.dirname(f).endswith("src")}
    parsed = _parse_regions({f: t for f, t in rs_text.items() if f not in whole})
    if parsed is None:
        return None
    # regions inside lib.rs/spec.rs (the prelude usually lives there) are also parsed, for fns/harnesses defined there
    p2 = _parse_regions(whole)
    if p2 is None:
        return None
    prelude = parsed[0] + "\n" + p2[0]
    fns = dict(parsed[1])
    harn = dict(parsed[2])
    for n, v in p2[1].items():
        if n in fns:
            return None
        fns[n] = v
    for n, v in p2[2].items():
        if n in harn:
            return None
        harn[n] = v
    names = [x for x in harnesses]
    if any(n not in harn for n in names):
        return None
    frame = h("frame", h(*other), h(*[x for f in sorted(whole) for x in (f, whole[f])]),
              h(*[x for f in sorted(parsed[3]) for x in (f, parsed[3][f])]), h(prelude))
    out = {}
    for n in names:
        hz = harn[n]
        stubs = set(hz["stubs"]) | set(re.findall(r"\bstub(?:_verified)?\s*\(\s*(\w+)", hz["src"]))
        stubs = {s for s in stubs if s in fns}
        # every region function named anywhere in what this harness runs and is not replaced by a stub
        seen, todo = set(), [hz["src"] + "\n" + (fns[hz["target"]]["full"] if hz["target"] in fns else "")]
        seen.add(hz["target"])
        todo_names = set(hz["uses"])
        while True:
            words = set()
            for t in todo:
                words |= set(WORD_RX.findall(t))
            todo = []
            new = ((words | todo_names) & set(fns)) - seen - stubs
            todo_names = set()
            if not new:
                break
            seen |= new
            todo = [fns[x]["full"] for x in new]
        parts = ["harness", n, h(hz["src"]), "meta", json.dumps([hz["target"], sorted(hz["stubs"]), sorted(hz["uses"])])]
        if hz["target"] in fns:
            parts += ["target", h(fns[hz["target"]]["full"])]
        for s in sorted(stubs):
            parts += ["stub:" + s, h(fns[s]["contract"])]
        for u in sorted(seen - {hz["target"]}):
            parts += ["use:" + u, h(fns[u]["full"])]
        out[n] = h(frame, *parts)
    return out


def compute_key(job):
    crate = Path(job["crate"])
    top, _ = toplevel(crate)
    head = ("proofq-v2", kani_version(), os.path.relpath(crate, top), json.dumps(kani_flags(job)),
            "exact" if job.get("exact", True) else "substr")
    hk = harness_keys(crate, sorted(set(job["harnesses"])), job.get("exact", True))
    if hk is None:
        return h(*head, "tree", json.dumps(sorted(set(job["harnesses"]))), tree_hash(crate))
    return h(*head, "harness-keys", json.dumps(sorted(hk.items())))


def contract_targets(crate):
    """harness fn name -> function named in #[kani::proof_for_contract(F)]."""
    top, files = source_files(crate)
    out = {}
    rx = re.compile(r"proof_for_contract\(\s*([\w:]+)\s*\)\s*\]\s*(?:#\[[^\n]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")
    for f in files:
        if f.endswith(".rs"):
            try:
                for fn, harness in rx.findall((top / f).read_text(errors="replace")):
                    out[harness] = fn
            except OSError:
                pass
    return out


# ---------------------------------------------------------------- Kani output parsing

CHECK_RX = re.compile(
    r"^Check \d+: (?P<check>\S+)\n\s+- Status: (?P<status>\w+)\n\s+- Description: \"(?P<desc>.*?)\"\n"
    r"\s+- Location: (?P<loc>[^\n]*)", re.M | re.S)
LOC_RX = re.compile(r"^(?P<file>.*?):(?P<line>\d+):(?P<col>\d+)(?: in function (?P<fn>.*))?$")


def source_line(crate, file, line):
    try:
        p = Path(file) if os.path.isabs(file) else Path(crate) / file
        return p.read_text(errors="replace").splitlines()[line - 1].strip()
    except Exception:
        return None


def split_harnesses(out):
    parts = re.split(r"^Checking harness (\S+?)\.\.\.[ \t]*$", out, flags=re.M)
    return list(zip(parts[1::2], parts[2::2]))


def parse_kani(out, crate):
    hs = []
    for name, body in split_harnesses(out):
        m = re.search(r"VERIFICATION:- (SUCCESSFUL|FAILED)", body)
        t = re.search(r"Verification Time: ([\d.]+)s", body)
        e = {"name": name, "status": None, "secs": round(float(t.group(1)), 2) if t else None,
             "failures": [], "counterexample": None}
        if m:
            e["status"] = "pass" if m.group(1) == "SUCCESSFUL" else "fail"
        for c in CHECK_RX.finditer(body):
            st, is_cover = c["status"], ".cover." in c["check"]
            bad = st in ("FAILURE", "UNDETERMINED") or (is_cover and st != "SATISFIED")
            if not bad:
                continue
            f = {"check": c["check"], "status": st, "description": " ".join(c["desc"].split()),
                 "file": None, "line": None, "column": None, "function": None, "source": None}
            lm = LOC_RX.match(c["loc"].strip())
            if lm:
                f.update(file=lm["file"], line=int(lm["line"]), column=int(lm["col"]), function=lm["fn"])
                f["source"] = source_line(crate, lm["file"], int(lm["line"]))
            if is_cover and st != "SATISFIED":
                # same rule as the kani-loop: an unreachable cover means a vacuous harness
                f["description"] = "unsatisfied cover (vacuous harness): " + f["description"]
                if e["status"] == "pass":
                    e["status"] = "fail"
            e["failures"].append(f)
        s = re.search(r"^SUMMARY:.*?(?=^VERIFICATION)", body, re.M | re.S)
        e["excerpt"] = s.group(0).strip()[:3000] if s and e["status"] == "fail" else None
        hs.append(e)
    return hs


def parse_playback(out):
    """harness -> {values: [{index, value, bytes}], playback_test}. values[i] is the
    i-th kani::any() the harness evaluates on the failing path."""
    res = {}
    for m in re.finditer(r"Concrete playback unit test for `([^`]+)`:\n```\n(.*?)\n```", out, re.S):
        vals = [{"index": i, "value": v.strip(), "bytes": [int(x) for x in b.replace(" ", "").split(",") if x]}
                for i, (v, b) in enumerate(re.findall(r"//\s*(.*?)\n\s*vec!\[([\d,\s]*)\]", m.group(2)))]
        res[m.group(1)] = {"values": vals, "playback_test": m.group(2)}
    return res


def parse_cargo_json(text):
    """Compiler errors from `cargo check --message-format=json` -> (errors, rendered text)."""
    errs, rendered = [], []
    for ln in text.splitlines():
        if not ln.startswith("{"):
            if ln.strip():
                rendered.append(ln)
            continue
        try:
            msg = json.loads(ln)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        d = msg["message"]
        if d.get("rendered"):
            rendered.append(d["rendered"].rstrip())
        if d.get("level") != "error" or d.get("message", "").startswith("aborting due to"):
            continue
        sp = next((s for s in d.get("spans", []) if s.get("is_primary")), None)
        errs.append({"message": d["message"], "code": (d.get("code") or {}).get("code"),
                     "file": sp["file_name"] if sp else None, "line": sp["line_start"] if sp else None,
                     "column": sp["column_start"] if sp else None, "rendered": (d.get("rendered") or "").rstrip()})
    return errs, "\n".join(rendered)


def tail(text, n=40, limit=4000):
    return "\n".join(text.splitlines()[-n:])[-limit:]


# ---------------------------------------------------------------- remote workers

LOCAL = "local"
SSH_OPTS = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=5",
            "-o", "ServerAliveCountMax=3"]
REMOTE_ROOT = ".cache/proofq-worker"     # under the worker's $HOME: src/<worktree>, target/<worktree>
REMOTE_ENV = ('export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"\n'
              'export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"\n')
END_MARK = "__PROOFQ_END__"
OOM_MARK = "__PROOFQ_OOM__"
# runs the command inside the scope and reports if the scope's cgroup had an out-of-memory kill
# (a scope is gone once it ends, so its Result cannot be read afterwards as for a local run)
PEAK_MARK = "__PROOFQ_PEAK__"
IN_SCOPE = ('cg="/sys/fs/cgroup$(cut -d: -f3- /proc/self/cgroup)"; pf=$(mktemp); echo 0 >"$pf"; '
            '( while :; do v=$(cat "$cg/memory.current" 2>/dev/null || echo 0); '
            '[ "${v:-0}" -gt "$(cat "$pf")" ] && echo "$v" >"$pf"; sleep 0.4; done ) 2>/dev/null & sp=$!; '
            '"$@"; rc=$?; kill $sp 2>/dev/null; '
            'pk=$(cat "$cg/memory.peak" 2>/dev/null); [ -z "$pk" ] || [ "$pk" = max ] && pk=$(cat "$pf"); rm -f "$pf"; '
            'printf \'\\n%s %s\\n\' ' + PEAK_MARK + ' "${pk:-0}"; '
            'n=$(awk \'$1=="oom_kill"{print $2}\' '
            '"/sys/fs/cgroup$(cut -d: -f3- /proc/self/cgroup)/memory.events" 2>/dev/null); '
            '[ "${n:-0}" != 0 ] && printf \'\\n%s\\n\' ' + OOM_MARK + '; exit $rc')
SCOPE_GLOBS = "'proofq-job-*.scope' 'proofq-check-*.scope'"


_workers = (None, [])


def workers_cfg():
    """Remote workers from workers.json, re-read when the file changes:
    {"workers": [{"name": "orin", "ssh": "mat@host", "slots": 1, "enabled": true, "memory_max": "3G"}]}"""
    global _workers
    try:
        st = WORKERS.stat()
        stamp = (st.st_mtime_ns, st.st_size)
    except OSError:
        stamp = None
    if stamp != _workers[0]:
        _workers = (stamp, _read_workers() if stamp else [])
    return [dict(w) for w in _workers[1]]


def _read_workers():
    try:
        raw = json.loads(WORKERS.read_text())
        out, seen = [], {LOCAL}
        for w in raw.get("workers", []):
            name = str(w.get("name") or w["ssh"])
            if name in seen or not re.fullmatch(r"[\w.@-]+", name):
                print(f"proofq: {WORKERS}: skipping worker with bad or duplicate name {name!r}", file=sys.stderr)
                continue
            seen.add(name)
            out.append({"name": name, "ssh": str(w["ssh"]), "slots": max(0, int(w.get("slots", 1))),
                        "enabled": bool(w.get("enabled", True)),
                        "memory_max": str(w["memory_max"]) if w.get("memory_max") else None,
                        "ssh_opts": [str(x) for x in w.get("ssh_opts", [])]})
        return out
    except FileNotFoundError:
        return []
    except Exception as e:  # a broken worker list must not stop the local queue
        print(f"proofq: ignoring bad worker list {WORKERS}: {e!r}", file=sys.stderr)
        return []


def ssh_argv(w):
    return ["ssh", *SSH_OPTS, *w.get("ssh_opts", []), w["ssh"]]


def ssh_script(w, script, timeout):
    """Run a bash script on a worker. It is sent on stdin, so the worker's login shell does not matter."""
    try:
        r = subprocess.run(ssh_argv(w) + ["bash", "-s"], input=REMOTE_ENV + script, capture_output=True,
                           text=True, timeout=timeout)
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired:
        return 255, "", "ssh: no answer within %ds" % timeout
    except OSError as e:
        return 255, "", f"ssh: {e}"


def last_line(text, default=""):
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    return lines[-1][:200] if lines else default


def probe_worker(w, c, clean):
    """-> (ok, note, home). A worker is usable if ssh works, its Kani version equals this host's
    and it has disk space. Also creates the work dirs, deletes worktree copies and target dirs
    idle for target_max_age_hours and, with clean, kills job scopes left over from a lost connection."""
    mins = int(c["target_max_age_hours"] * 60)
    script = (f"mkdir -p ~/{REMOTE_ROOT}/src ~/{REMOTE_ROOT}/target || exit 3\n"
              + (f"systemctl --user kill --signal=SIGKILL {SCOPE_GLOBS} 2>/dev/null\n" if clean else "")
              + f"find ~/{REMOTE_ROOT}/src ~/{REMOTE_ROOT}/target -mindepth 1 -maxdepth 1 -mmin +{mins} "
                f"-exec rm -rf {{}} + 2>/dev/null\n"
              + 'echo "home=$HOME"\necho "kani=$(cargo kani --version 2>/dev/null | head -1)"\n'
              + f"echo \"free_kb=$(df -Pk ~/{REMOTE_ROOT} | awk 'NR==2{{print $4}}')\"\n")
    rc, out, err = ssh_script(w, script, 60)
    info = dict(ln.split("=", 1) for ln in out.splitlines() if "=" in ln)
    if rc != 0 or "kani" not in info:
        return False, last_line(err, f"ssh exited {rc}"), None
    if info["kani"] != kani_version():
        return False, f"Kani version differs: worker has '{info['kani'] or 'none'}', this host '{kani_version()}'", None
    free = int(info.get("free_kb") or 0) / 1e6
    if free < c["worker_min_free_gb"]:
        return False, f"low disk: {free:.0f} GB free < worker_min_free_gb={c['worker_min_free_gb']}", None
    return True, "", info.get("home")


def remote_dirs(job, w):
    """Where a job's worktree copy, crate and target dir live on a worker (relative to its $HOME)."""
    top = Path(job["top"])
    name = re.sub(r"[^\w.-]", "_", top.name) + "-" + h(str(top))[:10]
    src = f"{REMOTE_ROOT}/src/{name}"
    return {"w": w, "src": src, "target": f"{REMOTE_ROOT}/target/{name}", "top": str(top),
            "crate": os.path.normpath(os.path.join(src, os.path.relpath(job["crate"], top)))}


def sync_to_worker(job, r):
    """rsync the files the cache key covers (git ls-files: tracked + untracked-not-ignored, plus
    Cargo.lock) to the worker; files that are no longer in that list are deleted there.
    -> None, or ("lost" | "sync", note)."""
    top, files = source_files(Path(job["crate"]))
    files = [f for f in files if "\n" not in f]
    dirs = {str(d) for f in files for d in Path(f).parents if str(d) != "."}

    def esc(x):
        return re.sub(r"([*?\[\\])", r"\\\1", x)
    flt = STATE / f"rsync-{job['id']}.filter"
    flt.write_text("".join(f"+ /{esc(d)}/\n" for d in sorted(dirs)) + "".join(f"+ /{esc(f)}\n" for f in files) + "- *\n")
    w = r["w"]
    cmd = ["rsync", "-a", "--delete", "--delete-excluded", "--timeout=30",
           "-e", " ".join(map(shlex.quote, ["ssh", *SSH_OPTS, *w.get("ssh_opts", [])])),
           f"--filter=. {flt}", f"{top}/", f"{w['ssh']}:{r['src']}/"]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=600, stdin=subprocess.DEVNULL)
        rc, err = p.returncode, p.stderr
    except subprocess.TimeoutExpired:
        rc, err = 30, "rsync: no progress for 600 s"
    finally:
        flt.unlink(missing_ok=True)
    if rc in (0, 24):
        return None
    return ("lost" if rc in (5, 10, 12, 30, 35, 255) else "sync"), last_line(err, f"rsync exited {rc}")


# ---------------------------------------------------------------- daemon

def systemctl(*a):
    return subprocess.run(["systemctl", "--user", *a], capture_output=True, text=True)


def kill_unit(unit):
    systemctl("kill", "--kill-whom=all", "--signal=SIGKILL", unit)


def wt_target(job):
    """One CARGO_TARGET_DIR per worktree. (A dir shared between worktrees is unsound: cargo
    gives same-named crates in different worktrees the same fingerprint, compared by mtime.)"""
    top = Path(job["top"])
    return TARGETS / f"{top.name}-{h(str(top))[:10]}"


def seed_dir(sid):
    return TARGETS / f"_seed-{sid[:16]}"


def find_lock(crate, top):
    d = Path(crate)
    while True:
        if (d / "Cargo.lock").exists():
            return d / "Cargo.lock"
        if d == Path(top) or d.parent == d:
            return None
        d = d.parent


def lock_packages(text):
    """(names of local/path packages, number of registry/git packages) in a Cargo.lock."""
    local, ext = [], 0
    for blk in text.split("[[package]]")[1:]:
        m = re.search(r'^name = "([^"]+)"', blk, re.M)
        if re.search(r"^source = ", blk, re.M):
            ext += 1
        elif m:
            local.append(m.group(1))
    return local, ext


def seed_id_for(crate, top):
    """Worktrees with the same Cargo.lock (and at least one external dependency) share a seed."""
    lock = find_lock(crate, top)
    if not lock:
        return None
    text = lock.read_text(errors="replace")
    if lock_packages(text)[1] == 0:
        return None
    return h("seed-v1", kani_version(), text)


def strip_local(root, names):
    """Remove the build state of local crates from a copied target dir, so that only
    registry/git dependencies are reused and every local crate is rebuilt from source."""
    pats = sorted({n for x in names for n in (x, x.replace("-", "_"))})
    rx = re.compile(r"^(%s)(-[0-9a-f]{16})?$" % "|".join(map(re.escape, pats))) if pats else None
    for d, dirs, _ in os.walk(root):
        parent = os.path.basename(d)
        for x in list(dirs):
            if x == "incremental" or (rx and parent in ("build", ".fingerprint") and rx.match(x)):
                shutil.rmtree(os.path.join(d, x), ignore_errors=True)
                dirs.remove(x)


class Daemon:
    def __init__(self):
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.jobs = {}        # id -> job dict, for jobs that are not done
        self.done = set()
        self.active = {}      # id -> {"phase": "check"|"kani", "cancel": Event, "thread": Thread}
        self.cancel_why = {}
        self.peak = 0
        self.paused = None
        self.leaders = {}     # seed id -> job id currently building the first copy of the dependencies
        self.busy_dirs = set()
        self.last_clean = 0
        self.hosts = {}       # remote worker name -> {"up", "note", "checked", "ssh", "home"}
        self.peaks = {}       # host name -> most Kani jobs seen running at once

    def log(self, msg):
        print(msg, flush=True)

    def save(self, job):
        write_json(JOBS / f"{job['id']}.json", job)

    def finish(self, job, result):
        result.setdefault("harnesses", [])
        job.update(state="done", finished_at=now(), result=result)
        result["wall_secs"] = round(job["finished_at"] - job["submitted_at"], 2)
        with self.lock:
            self.save(job)
            self.jobs.pop(job["id"], None)
            self.done.add(job["id"])
            self.active.pop(job["id"], None)
            n = sum(1 for a in self.active.values() if a["phase"] == "kani")
        self.log(f"done  {job['id']} {result['status']} stage={result.get('stage')} "
                 f"kani_secs={result.get('kani_secs')} who={job['who']} host={result.get('host') or '-'} kani_running={n}")

    # -- scoped execution

    def run_remote(self, unit, cmd, logf, timeout, mem, cancel, r, stats=None):
        """run_scoped on a worker: the same systemd scope, started over ssh, output streamed into
        the same log. why is "lost" when the connection or the worker went away before the
        command reported its exit code."""
        w, q = r["w"], shlex.quote
        # a scope left by an earlier attempt is killed, and is gone before the unit name is used again
        # (no --kill-whom: systemd 249 on the Jetsons does not have it, and "all" is the default)
        kill = (f"systemctl --user kill --signal=SIGKILL {unit} 2>/dev/null\n"
                f"for _ in $(seq 50); do [ \"$(systemctl --user show {unit} -p ActiveState --value 2>/dev/null)\" "
                f"= active ] || break; sleep 0.2; done\n"
                f"systemctl --user reset-failed {unit} 2>/dev/null\n")
        script = (REMOTE_ENV + "{\n"
                  + f'export CARGO_TARGET_DIR="$HOME"/{q(r["target"])} CARGO_TERM_COLOR=never\n'
                  + f'mkdir -p "$CARGO_TARGET_DIR"; touch "$CARGO_TARGET_DIR" "$HOME"/{q(r["src"])}\n'
                  + f'cd "$HOME"/{q(r["crate"])} || {{ printf \'\\n%s 1 nocd\\n\' {END_MARK}; exit 0; }}\n' + kill
                  + f"systemd-run --user --scope --quiet --unit={unit} --slice=proofq.slice -p MemoryMax={q(str(mem))} "
                    f"-p MemorySwapMax=0 -p RuntimeMaxSec={int(timeout) + 30} -- nice -n 10 bash -c {q(IN_SCOPE)} proofq "
                    f"{' '.join(map(q, cmd))} "
                    f"</dev/null 2>&1\nrc=$?\n"
                  + f"res=$(systemctl --user show {unit} -p Result --value 2>/dev/null)\n" + kill
                  + f"printf '\\n%s %s %s\\n' {END_MARK} \"$rc\" \"$res\"\nexit 0\n}}\n")
        logf.flush()
        start = os.fstat(logf.fileno()).st_size
        t0 = now()
        proc = subprocess.Popen(ssh_argv(w) + ["bash", "-s"], stdin=subprocess.PIPE, stdout=logf,
                                stderr=subprocess.STDOUT)
        try:
            proc.stdin.write(script.encode())
            proc.stdin.close()
        except OSError:
            pass
        why = None
        while True:
            try:
                proc.wait(0.25)
                break
            except subprocess.TimeoutExpired:
                if now() - t0 > timeout:
                    why = "timeout"
                elif cancel.is_set():
                    why = "cancelled"
                elif self.stop.is_set():
                    why = "shutdown"
                if why:
                    ssh_script(w, kill, 20)
                    proc.kill()
                    proc.wait()
                    break
        secs = round(now() - t0, 2)
        # take the end marker out of the log and turn the worker's paths into this host's
        with open(logf.name, "rb+") as f:
            f.seek(start)
            data = f.read()
            m = re.search(rb"\n?%s (-?\d+) ?(\S*)\s*\Z" % END_MARK.encode(), data)
            body = data[:m.start()] if m else data
            pm = list(re.finditer(rb"\n%s (\d+)\n" % PEAK_MARK.encode(), body[-600:]))
            if pm:
                if stats is not None:
                    stats["peak"] = int(pm[-1].group(1))
                cut = len(body) - min(len(body), 600) + pm[-1].start()
                body = body[:cut] + body[cut + len(pm[-1].group(0)) - 1:]
            oom = b"\n" + OOM_MARK.encode() + b"\n"
            if oom in body[-200:]:
                body, m_oom = body[:-200] + body[-200:].replace(oom, b"\n"), True
            else:
                m_oom = False
            if r.get("home"):
                body = body.replace(os.fsencode(f"{r['home']}/{r['src']}"), os.fsencode(r["top"]))
            f.seek(start)
            f.truncate()
            f.write(body)
        rc = int(m.group(1)) if m else 255
        if not why:
            if not m:
                ll = last_line(data.decode(errors="replace"), "")
                why, r["lost_note"] = "lost", ll if re.match(r"(ssh|kex_exchange|client_loop|Connection|Timeout)\b", ll) \
                    else "ssh connection lost"
            elif m_oom or m.group(2) == b"oom-kill":
                why = "oom"
        return rc, why, secs

    def run_scoped(self, unit, cmd, cwd, env, logf, timeout, mem, cancel, remote=None, stats=None):
        """stats (optional dict) gets "peak": the scope's peak memory in bytes, when it could be read."""
        if remote:
            return self.run_remote(unit, cmd, logf, timeout, mem, cancel, remote, stats)
        full = ["systemd-run", "--user", "--scope", "--quiet", f"--unit={unit}", "--slice=proofq.slice",
                "-p", f"MemoryMax={mem}", "-p", "MemorySwapMax=0", "-p", f"RuntimeMaxSec={int(timeout) + 30}",
                "--", "nice", "-n", "10", *cmd]
        t0 = now()
        proc = subprocess.Popen(full, cwd=cwd, env=env, stdout=logf, stderr=subprocess.STDOUT,
                                stdin=subprocess.DEVNULL)
        why, cgf, peak = None, None, 0
        while True:
            try:
                rc = proc.wait(0.25)
                break
            except subprocess.TimeoutExpired:
                if stats is not None:
                    try:
                        if cgf is None:     # systemd-run moves itself into the scope, then execs the command
                            cg = Path(f"/proc/{proc.pid}/cgroup").read_text().strip().split(":", 2)[-1]
                            if cg.endswith(unit):
                                cgf = Path("/sys/fs/cgroup" + cg)
                        if cgf is not None:
                            for nm in ("memory.peak", "memory.current"):
                                try:
                                    peak = max(peak, int((cgf / nm).read_text()))
                                    break
                                except (OSError, ValueError):
                                    pass
                    except OSError:
                        pass
                if now() - t0 > timeout:
                    why = "timeout"
                elif cancel.is_set():
                    why = "cancelled"
                elif self.stop.is_set():
                    why = "shutdown"
                if why:
                    kill_unit(unit)
                    proc.kill()
                    rc = proc.wait()
                    break
        if not why and systemctl("show", unit, "-p", "Result", "--value").stdout.strip() == "oom-kill":
            why = "oom"
        kill_unit(unit)  # no stragglers
        systemctl("reset-failed", unit)
        if stats is not None and peak:
            stats["peak"] = peak
        return rc, why, round(now() - t0, 2)

    def target_dir(self, job, c):
        d, sid = wt_target(job), job.get("seed_id")
        seed = seed_dir(sid) if sid and c["share_deps"] else None
        if seed and not d.exists() and seed.exists():
            with self.lock:
                self.busy_dirs.add(seed)
            try:
                (seed / ".proofq-used").write_text(str(now()))
                tmp = d.with_name(d.name + ".tmp")
                shutil.rmtree(tmp, ignore_errors=True)
                if subprocess.run(["cp", "-a", str(seed), str(tmp)]).returncode == 0:
                    os.rename(tmp, d)
                    job["seeded"] = True
            finally:
                with self.lock:
                    self.busy_dirs.discard(seed)
        d.mkdir(parents=True, exist_ok=True)
        (d / ".proofq-used").write_text(str(now()))
        return d

    def make_seed(self, job):
        sid = job["seed_id"]
        dst, src = seed_dir(sid), wt_target(job)
        if dst.exists() or not src.exists():
            return
        lock = find_lock(job["crate"], job["top"])
        local = lock_packages(lock.read_text(errors="replace"))[0] if lock else []
        tmp = dst.with_name(dst.name + ".tmp")
        shutil.rmtree(tmp, ignore_errors=True)
        if subprocess.run(["cp", "-a", str(src), str(tmp)]).returncode != 0:
            shutil.rmtree(tmp, ignore_errors=True)
            return
        strip_local(tmp, local)
        (tmp / ".proofq-used").write_text(str(now()))
        os.rename(tmp, dst)
        self.log(f"seed {dst.name} created from {job['id']} (dependencies are now shared; local crates stripped: {local})")

    def interrupted(self, job, why, stage, secs_key, secs):
        """Common handling for a run that did not finish on its own. True if handled."""
        if why == "shutdown":
            job.update(state="queued")
            job.pop("started_at", None)
            job.pop("host", None)
            with self.lock:
                self.save(job)
                self.active.pop(job["id"], None)
            return True
        if why == "cancelled":
            info = self.cancel_why.pop(job["id"], {})
            st = "superseded" if info.get("reason") == "superseded" else "cancelled"
            self.finish(job, {"status": st, "stage": stage, secs_key: secs, "superseded_by": info.get("by"),
                              "note": f"{st} during {stage}"})
            return True
        return False

    def back_to_queue(self, job, w, why, note, mark_down):
        """A job that a worker could not give a verdict for goes back to the queue, ahead of
        newer jobs (it keeps its seq). Unless the worker was simply unreachable, it then runs
        on this host, so that timeouts and memory limits mean the same as for a local run."""
        n = job.get("requeues", 0) + 1
        job.update(state="ready", requeues=n, requeue_note=f"{why} on {w['name']}: {note}")
        if not mark_down or n >= 3:
            job["local_only"] = True
        job.pop("started_at", None)
        job.pop("host", None)
        try:
            with open(LOGS / f"{job['id']}.log", "a") as f:
                f.write(f"\n=== proofq: {why} on worker {w['name']} ({note}); job put back in the queue"
                        f"{' for a local run' if job.get('local_only') else ''} ===\n")
        except OSError:
            pass
        if mark_down:
            self.set_host(w, False, note)
        with self.lock:
            self.save(job)
            self.active.pop(job["id"], None)
        self.log(f"requeue {job['id']} who={job['who']}: {why} on {w['name']} ({note})"
                 f"{' -> local only' if job.get('local_only') else ''}")

    def set_host(self, w, up, note, home=None):
        with self.lock:
            old = self.hosts.get(w["name"]) or {}
            self.hosts[w["name"]] = {"up": up, "note": note, "checked": now(), "ssh": w["ssh"], "home": home}
        if old.get("up") != up or old.get("ssh") != w["ssh"] or (not up and old.get("note") != note):
            self.log(f"worker {w['name']} ({w['ssh']}) " + (f"up: {w['slots']} slots" if up else f"DOWN: {note}"))

    def monitor(self):
        """Probes remote workers: down ones every worker_recheck_secs until they answer again,
        up ones while they are idle (a running job notices a dead connection by itself)."""
        while not self.stop.is_set():
            try:
                c = cfg()
                for w in workers_cfg():
                    if not w["enabled"] or w["slots"] < 1 or self.stop.is_set():
                        continue
                    with self.lock:
                        st = self.hosts.get(w["name"]) or {}
                        busy = any(a.get("host") == w["name"] for a in self.active.values())
                    known = st.get("ssh") == w["ssh"]
                    if known and (busy or now() - st["checked"] < c["worker_recheck_secs"]):
                        continue
                    ok, note, home = probe_worker(w, c, clean=not busy and not (known and st.get("up")))
                    self.set_host(w, ok, note, home)
            except Exception as e:
                self.log(f"worker monitor error: {e!r}")
            self.stop.wait(1)

    def host_table(self, c):
        """Hosts in the order jobs fill them: this one first, then workers.json order."""
        rows = [{"name": LOCAL, "slots": c["workers"], "state": "up", "note": "", "ssh": None,
                 "memory_max": c["memory_max"], "worker": None}]
        for w in workers_cfg():
            st = self.hosts.get(w["name"]) or {}
            mem = w["memory_max"] or c["worker_memory_max"]
            if not w["enabled"]:
                state, note = "disabled", ""
            elif w["slots"] < 1:
                state, note = "disabled", "slots: 0"
            elif st.get("ssh") != w["ssh"]:
                state, note = "down", "not checked yet"
            else:
                state, note = ("up" if st["up"] else "down"), st.get("note", "")
            rows.append({"name": w["name"], "slots": w["slots"], "state": state, "note": note, "ssh": w["ssh"],
                         "memory_max": mem, "checked": st.get("checked"),
                         "worker": dict(w, memory_max=mem, home=st.get("home"))})
        return rows

    def cache_hit(self, job, key):
        if job.get("no_cache"):
            return False
        hit = read_json(RESULTS / f"{key}.json")
        if not hit:
            return False
        res = dict(hit)
        res.update(cached=True, cached_from=hit.get("job"), orig_wall_secs=hit.get("wall_secs"))
        job["key"] = key
        self.finish(job, res)
        return True

    def store(self, job, key, result):
        if compute_key(job) != key:
            result["note"] = "sources changed during the run; result not cached"
            return
        entry = dict(result, job=job["id"], key=key, at=now())
        try:
            shutil.copyfile(LOGS / f"{job['id']}.log", RESULTS / f"{key}.log")
        except OSError:
            pass
        write_json(RESULTS / f"{key}.json", entry)

    # -- phase 1: cargo check / cargo test, outside the Kani slots

    def precheck(self, job, cancel):
        try:
            self._precheck(job, cancel)
        except Exception as e:
            self.finish(job, {"status": "error", "stage": "check", "note": f"proofq internal error: {e!r}"})
        finally:
            with self.lock:
                for k in [k for k, v in self.leaders.items() if v == job["id"]]:
                    del self.leaders[k]

    def _precheck(self, job, cancel):
        c, jid, crate = cfg(), job["id"], job["crate"]
        if not (Path(crate) / "Cargo.toml").exists():
            return self.finish(job, {"status": "error", "stage": "check", "note": f"no Cargo.toml in {crate}"})
        key = compute_key(job)
        if self.cache_hit(job, key):
            return
        job["key"] = key
        env = tool_env()
        env["CARGO_TARGET_DIR"] = str(self.target_dir(job, c))
        env["CARGO_TERM_COLOR"] = "never"
        log = LOGS / f"{jid}.log"
        t0 = now()

        def seed_step():
            """First job with this Cargo.lock: build the dependencies (also for Kani) and publish
            them as a seed. Jobs from other worktrees with the same lock wait for this."""
            if self.leaders.get(job.get("seed_id")) != jid:
                return None
            with open(log, "a") as f:
                f.write("=== proofq: first build of this Cargo.lock: cargo kani --only-codegen, to share the dependencies ===\n")
                f.flush()
                _, why, _ = self.run_scoped(f"proofq-check-{jid}.scope", ["cargo", "kani", *kani_flags(job), "--only-codegen"],
                                            crate, env, f, c["seed_build_timeout"], c["memory_max"], cancel)
            if why in (None, "timeout"):
                self.make_seed(job)
            return why

        if c["precheck"] and job.get("check", True):
            raw = LOGS / f"{jid}.check.raw"
            with open(raw, "w") as f:
                rc, why, _ = self.run_scoped(f"proofq-check-{jid}.scope", ["cargo", "check", "--message-format=json"],
                                             crate, env, f, c["precheck_check_timeout"], c["precheck_memory_max"], cancel)
            text = raw.read_text(errors="replace")
            raw.unlink(missing_ok=True)
            errs, rendered = parse_cargo_json(text)
            with open(log, "a") as f:
                f.write(f"=== proofq: cargo check (exit {rc}) ===\n{rendered}\n")
            secs = round(now() - t0, 2)
            if self.interrupted(job, why, "check", "check_secs", secs):
                return
            if why:
                return self.finish(job, {"status": "error", "stage": "check", "check_secs": secs,
                                         "note": f"cargo check: {why}", "excerpt": tail(rendered)})
            if rc != 0:
                if self.interrupted(job, seed_step(), "check", "check_secs", secs):
                    return
                res = {"status": "fail", "stage": "check", "check_secs": secs, "compile_errors": errs,
                       "excerpt": "\n\n".join(e["rendered"] for e in errs)[:6000] or tail(rendered),
                       "note": "cargo check failed; no Kani slot was used"}
                self.store(job, key, res)
                return self.finish(job, res)
        if c["precheck"] and job.get("test", True) and c["precheck_test"]:
            start = log.stat().st_size if log.exists() else 0
            with open(log, "a") as f:
                f.write("=== proofq: cargo test ===\n")
                f.flush()
                rc, why, _ = self.run_scoped(f"proofq-check-{jid}.scope", ["cargo", "test"], crate, env, f,
                                             c["precheck_test_timeout"], c["precheck_memory_max"], cancel)
            secs = round(now() - t0, 2)
            if self.interrupted(job, why, "test", "check_secs", secs):
                return
            if why:
                job["precheck_note"] = f"cargo test skipped ({why} after {c['precheck_test_timeout']}s limit)"
            elif rc != 0:
                with open(log, errors="replace") as f:
                    f.seek(start)
                    out = f.read()
                m = re.search(r"^failures:\n.*", out, re.M | re.S)
                return self.finish(job, {"status": "fail", "stage": "test", "check_secs": secs,
                                         "excerpt": (m.group(0)[:4000] if m else tail(out)),
                                         "note": "cargo test failed; no Kani slot was used (not cached)"})
        if self.interrupted(job, seed_step(), "check", "check_secs", round(now() - t0, 2)):
            return
        job.update(state="ready", check_secs=round(now() - t0, 2))
        with self.lock:
            self.save(job)
            self.active.pop(jid, None)

    # -- phase 2: Kani

    def kani(self, job, cancel):
        try:
            self._kani(job, cancel)
        except Exception as e:
            self.finish(job, {"status": "error", "stage": "kani", "note": f"proofq internal error: {e!r}"})

    def _kani(self, job, cancel):
        c, jid, crate = cfg(), job["id"], job["crate"]
        if not (Path(crate) / "Cargo.toml").exists():
            return self.finish(job, {"status": "error", "stage": "kani", "note": f"no Cargo.toml in {crate}"})
        key = compute_key(job)
        if self.cache_hit(job, key):
            return
        job["key"] = key
        log = LOGS / f"{jid}.log"
        with self.lock:
            w = (self.active.get(jid) or {}).get("worker")
        env, remote, mem, host = None, None, c["memory_max"], LOCAL
        if w:
            # remote: copy the worktree, then the same cargo check + cargo kani in a scope there
            remote, mem, host = dict(remote_dirs(job, w), home=w.get("home")), w["memory_max"], w["name"]
            bad = sync_to_worker(job, remote)
            if cancel.is_set() or self.stop.is_set():
                if self.interrupted(job, "cancelled" if cancel.is_set() else "shutdown", "kani", "kani_secs", 0):
                    return
            if bad:
                return self.back_to_queue(job, w, "rsync failed" if bad[0] == "sync" else "worker unreachable",
                                          bad[1], bad[0] == "lost")
            if c["precheck"] and job.get("check", True):
                with open(log, "a") as f:
                    f.write(f"=== proofq: [{host}] cargo check ===\n")
                    f.flush()
                    rc, why, secs = self.run_scoped(f"proofq-check-{jid}.scope", ["cargo", "check"], crate, None, f,
                                                    c["worker_check_timeout"], mem, cancel, remote)
                if self.interrupted(job, why, "kani", "kani_secs", secs):
                    return
                if why == "lost":
                    return self.back_to_queue(job, w, "worker unreachable", remote.get("lost_note", ""), True)
                if why or rc != 0:
                    return self.back_to_queue(job, w, f"cargo check {why or 'failed'}", f"exit {rc} after {secs}s", False)
        else:
            env = tool_env()
            env["CARGO_TARGET_DIR"] = str(self.target_dir(job, c))
            env["CARGO_TERM_COLOR"] = "never"
            pk = job_peak_mb(job)       # a harness that peaked above the limit before gets more room (up to big_memory_max_gb)
            if pk * 1.25 > mem_mb(mem):
                mem = f"{int(min(pk * 1.25, max(c['big_memory_max_gb'] * 1024, mem_mb(mem))))}M"
        base = ["cargo", "kani", *kani_flags(job)]
        stats = {}

        def harness_args(names):
            a = []
            for n in names:
                a += ["--harness", n]
            return a + (["--exact"] if names and job.get("exact", True) else [])

        cmd = base + harness_args(job["harnesses"])
        start = log.stat().st_size if log.exists() else 0
        with open(log, "a") as f:
            f.write(f"=== proofq: {'[' + host + '] ' if remote else ''}{' '.join(cmd)} ===\n")
            f.flush()
            rc, why, secs = self.run_scoped(f"proofq-job-{jid}.scope", cmd, crate, env, f, job["timeout"],
                                            mem, cancel, remote, stats)
        if self.interrupted(job, why, "kani", "kani_secs", secs):
            return
        if why == "lost":
            return self.back_to_queue(job, w, "worker unreachable", remote.get("lost_note", ""), True)
        with open(log, errors="replace") as f:
            f.seek(start)
            out = f.read()
        hs = parse_kani(out, crate)
        res = {"stage": "kani", "kani_secs": secs, "check_secs": job.get("check_secs"), "harnesses": hs,
               "cached": False, "host": host}
        if job.get("requeues"):
            res.update(requeues=job["requeues"], requeue_note=job.get("requeue_note"))
        if job.get("precheck_note"):
            res["precheck_note"] = job["precheck_note"]
        seen = {x["name"] for x in hs}
        for n in job["harnesses"]:
            if n not in seen:
                hs.append({"name": n, "status": "not_run", "secs": None, "failures": [], "counterexample": None})
        if why == "timeout":
            for x in hs:
                x["status"] = x["status"] or "timeout"
            res.update(status="timeout", note=f"killed after the {job['timeout']}s timeout")
        elif why == "oom":
            for x in hs:
                x["status"] = x["status"] if x["status"] == "pass" else "error"
            res.update(status="error", note=f"out of memory: the job exceeded MemoryMax={mem}")
        elif not seen:
            if re.search(r"^error(\[E\d+\])?:", out, re.M) and "could not compile" in out:
                errs = re.findall(r"^error(?:\[E\d+\])?:.*?(?=^\S|\Z)", out, re.M | re.S)
                res.update(status="fail", stage="kani-compile", note="the crate does not compile under Kani",
                           excerpt="\n".join(errs)[:6000] or tail(out))
            else:
                res.update(status="error", note="Kani produced no harness results", excerpt=tail(out))
        elif any(x["status"] == "fail" for x in hs):
            res["status"] = "fail"
        elif any(x["status"] != "pass" for x in hs) or rc != 0:
            for x in hs:
                x["status"] = x["status"] or "error"
            res.update(status="error", note=f"Kani exited {rc} without a verdict for every harness",
                       excerpt=tail(out))
        else:
            res["status"] = "pass"
        peak_mb = round(stats["peak"] / 1048576) if stats.get("peak") else None
        res["peak_mb"] = peak_mb
        if res["stage"] == "kani" and (res["status"] in ("pass", "fail") or why in ("oom", "timeout")):
            self.record(job, hs, host, bool(remote), peak_mb, mem_mb(mem), res["status"] in ("pass", "fail"), why)
        if remote and (res["status"] not in ("pass", "fail") or res["stage"] != "kani"):
            # only a Kani verdict is taken from a worker; a timeout, an out-of-memory kill or a
            # build problem there may not happen on this host, so the job is run again here
            return self.back_to_queue(job, w, why or res["status"], res.get("note", ""), False)

        failing = [x for x in hs if x["status"] == "fail" and any(".cover." not in f["check"] for f in x["failures"])]
        if failing and c["counterexample"] and job.get("cex", True) and why is None:
            budget = max(5, min(c["counterexample_timeout"], job["timeout"]))
            cmd2 = base + ["-Z", "concrete-playback", "--concrete-playback=print"]
            for x in failing:
                cmd2 += ["--harness", x["name"]]
            cmd2.append("--exact")
            start2 = log.stat().st_size
            with open(log, "a") as f:
                f.write(f"\n=== proofq: counterexample pass: {' '.join(cmd2)} ===\n")
                f.flush()
                _, why2, secs2 = self.run_scoped(f"proofq-job-{jid}.scope", cmd2, crate, env, f, budget,
                                                 mem, cancel, remote)
            res["counterexample_secs"] = secs2
            if self.interrupted(job, why2, "kani", "kani_secs", secs + secs2):
                return
            if why2 == "lost":
                return self.back_to_queue(job, w, "worker unreachable", remote.get("lost_note", ""), True)
            if remote and why2:     # no counterexample from the worker: get the whole result here
                return self.back_to_queue(job, w, why2, f"counterexample pass stopped after {secs2}s", False)
            with open(log, errors="replace") as f:
                f.seek(start2)
                pb = parse_playback(f.read())
            for x in failing:
                x["counterexample"] = pb.get(x["name"])
            if why2:
                res["counterexample_note"] = f"counterexample pass stopped: {why2} after {secs2}s"
        if res["status"] in ("pass", "fail") and not res.get("counterexample_note"):
            self.store(job, key, res)   # a failure whose counterexample pass was cut short is not cached
        self.finish(job, res)

    def record(self, job, hs, host, remote, peak_mb, mem_limit_mb, verdict, why):
        """Learn from a Kani run: time and peak memory per harness name (history.json). A run killed
        for memory counts as peaking at its limit; one that a worker could not finish is marked needs_local."""
        names = job["harnesses"]
        if not names:
            return
        secs = {x["name"].split("::")[-1]: x.get("secs") for x in hs if x.get("secs") is not None}
        try:
            with self.lock:
                data = read_json(HISTORY) or {}
                hz = data.setdefault("harnesses", {})
                for n in names:
                    e = hz.setdefault(n, {"runs": 0})
                    t = secs.get(n.split("::")[-1]) if verdict else None
                    if t is None and len(names) == 1:
                        t = job.get("timeout") if why == "timeout" else None
                    if t is not None:
                        k = "remote_secs" if remote else "local_secs"
                        e[k] = round(t, 2)
                    pk = peak_mb if len(names) == 1 else None      # a multi-harness job's peak belongs to no single harness
                    if why == "oom":
                        pk = max(pk or 0, mem_limit_mb)
                    if pk is not None:
                        e["peak_mb"] = round(max(pk, 0.7 * (e.get("peak_mb") or 0)))
                    if remote and why in ("oom", "timeout"):
                        e["needs_local"] = True
                    e.update(runs=e.get("runs", 0) + 1, at=now(), host=host)
                write_json(HISTORY, data)
        except Exception as ex:
            self.log(f"history not updated: {ex!r}")

    # -- scheduling

    def recover(self):
        for j in all_jobs():
            if j["state"] in ("checking", "running"):
                for kind in ("job", "check"):
                    kill_unit(f"proofq-{kind}-{j['id']}.scope")
                    systemctl("reset-failed", f"proofq-{kind}-{j['id']}.scope")
                j["state"] = "queued" if j["state"] == "checking" else "ready"
                j.pop("started_at", None)
                j.pop("host", None)     # a scope left on a worker is killed by the first probe
                self.save(j)
                self.log(f"requeued {j['id']} after a daemon restart")

    def scan(self):
        for f in JOBS.glob("*.json"):
            jid = f.stem
            if jid in self.done or jid in self.jobs:
                continue
            j = read_json(f)
            if not j:
                continue
            if j["state"] == "done":
                self.done.add(jid)
            else:
                self.jobs[jid] = j
        for f in JOBS.glob("*.cancel"):
            jid = f.stem
            info = read_json(f) or {}
            f.unlink(missing_ok=True)
            with self.lock:
                j, a = self.jobs.get(jid), self.active.get(jid)
            if not j:
                continue
            if a:
                self.cancel_why[jid] = info
                a["cancel"].set()
            else:
                st = "superseded" if info.get("reason") == "superseded" else "cancelled"
                self.finish(j, {"status": st, "stage": "queue", "superseded_by": info.get("by"),
                                "note": f"{st} while waiting"})

    def pick_host(self, j, hosts, load, big_waiting, c):
        """Learned routing. Small harnesses (peak below route_small_peak_gb in history) go to the
        remote workers; big ones, and any job that a worker could not finish, run on this host;
        a small one spills over to a local slot only when that leaves a slot for every waiting big
        one. A harness without history takes the first free slot, this host first."""
        free = [x for x in hosts if x["state"] == "up" and load.get(x["name"], 0) < x["slots"]]
        local = [x for x in free if not x["worker"]]
        cls = route_class(j, c)
        if cls == "big":
            return local[0] if local else None
        if cls == "small":
            pk = job_peak_mb(j)
            remote = [x for x in free if x["worker"] and mem_mb(x["memory_max"]) >= pk * 1.1]
            if remote:
                return remote[0]
            spare = sum(x["slots"] - load.get(x["name"], 0) for x in local)
            return local[0] if local and spare > big_waiting else None
        return free[0] if free else None

    def dispatch(self, c):
        free = shutil.disk_usage(CACHE).free / 1e9
        self.paused = f"low disk: {free:.0f} GB free < min_free_gb={c['min_free_gb']}" if free < c["min_free_gb"] else None
        if self.paused or self.stop.is_set():
            return
        with self.lock:
            hosts = self.host_table(c)
            load = {}
            for a in self.active.values():
                if a["phase"] == "kani":
                    load[a["host"]] = load.get(a["host"], 0) + 1
            busy_tops = {self.jobs[i]["top"] for i in self.active if i in self.jobs}
            busy_keys = {self.jobs[i].get("key") for i, a in self.active.items() if a["phase"] == "kani" and i in self.jobs}
            n_check = sum(1 for a in self.active.values() if a["phase"] == "check")
            pending = sorted((j for i, j in self.jobs.items() if i not in self.active), key=order_key)
        big_waiting = sum(1 for j in pending if j["state"] in ("queued", "checking", "ready") and route_class(j, c) == "big")
        for j in pending:
            if j["top"] in busy_tops:
                continue  # one job per worktree at a time
            host = None
            if j["state"] == "ready":
                if j.get("key") in busy_keys and not j.get("no_cache"):
                    continue  # identical proof already running: wait and take its cached result
                host = self.pick_host(j, hosts, load, big_waiting, c)
                if not host:
                    continue  # every slot it may use is taken
                if route_class(j, c) == "big":
                    big_waiting -= 1
                phase, target = "kani", self.kani
                load[host["name"]] = load.get(host["name"], 0) + 1
                j.update(state="running", started_at=now(), host=host["name"])
                busy_keys.add(j.get("key"))
            elif j["state"] == "queued" and n_check < c["precheck_workers"]:
                sid = j.get("seed_id") if c["share_deps"] else None
                if sid and not wt_target(j).exists() and not seed_dir(sid).exists():
                    with self.lock:
                        if sid in self.leaders:
                            continue  # another job is building these dependencies; wait for its seed
                        self.leaders[sid] = j["id"]
                phase, target, n_check = "check", self.precheck, n_check + 1
                j.update(state="checking", check_started_at=now())
            else:
                continue
            busy_tops.add(j["top"])
            cancel = threading.Event()
            t = threading.Thread(target=target, args=(j, cancel), daemon=True)
            with self.lock:
                self.save(j)
                self.active[j["id"]] = {"phase": phase, "cancel": cancel, "thread": t,
                                        "host": host["name"] if host else None,
                                        "worker": host["worker"] if host else None}
                self.peak = max(self.peak, sum(load.values()))
                if host:
                    self.peaks[host["name"]] = max(self.peaks.get(host["name"], 0), load[host["name"]])
            if phase == "kani":
                total = sum(x["slots"] for x in hosts if x["state"] == "up")
                self.log(f"start {j['id']} who={j['who']} prio={j['priority']} host={host['name']} "
                         f"({load[host['name']]}/{host['slots']}) kani_running={sum(load.values())}/{total}")
            t.start()

    def cleanup(self, c):
        if now() - self.last_clean < 120:
            return
        self.last_clean = now()
        with self.lock:
            idle = not self.active
            busy = {wt_target(self.jobs[i]) for i in self.active if i in self.jobs} | set(self.busy_dirs)
        dirs = []
        for d in TARGETS.iterdir():
            if not d.is_dir():
                continue
            if d.name.endswith(".tmp"):
                if idle:
                    shutil.rmtree(d, ignore_errors=True)
                continue
            try:
                used = float((d / ".proofq-used").read_text())
            except Exception:
                used = d.stat().st_mtime
            kb = subprocess.run(["du", "-sk", str(d)], capture_output=True, text=True).stdout.split()
            dirs.append((used, d, int(kb[0]) / 1e6 if kb else 0))
        total = sum(x[2] for x in dirs)
        for used, d, gb in sorted(dirs):
            old = now() - used > c["target_max_age_hours"] * 3600
            if d not in busy and (old or total > c["target_max_total_gb"]):
                shutil.rmtree(d, ignore_errors=True)
                self.log(f"cleaned target dir {d.name} ({gb:.2f} GB, {'idle too long' if old else 'over the size cap'})")
                total -= gb
        cut = now() - c["keep_jobs_days"] * 86400
        for f in JOBS.glob("*.json"):
            if f.stem in self.done and f.stat().st_mtime < cut:
                f.unlink(missing_ok=True)
                (LOGS / f"{f.stem}.log").unlink(missing_ok=True)
                self.done.discard(f.stem)
        cut = now() - c["keep_cache_days"] * 86400
        for f in RESULTS.iterdir():
            if f.stat().st_mtime < cut:
                f.unlink(missing_ok=True)

    def run(self):
        ensure_dirs()
        signal.signal(signal.SIGTERM, lambda *_: self.stop.set())
        signal.signal(signal.SIGINT, lambda *_: self.stop.set())
        self.recover()
        self.log(f"proofq daemon up: workers={cfg()['workers']} state={STATE} kani='{kani_version()}' "
                 f"remote workers: {[(w['name'], w['slots']) for w in workers_cfg() if w['enabled']]} ({WORKERS})")
        threading.Thread(target=self.monitor, daemon=True).start()
        beat = 0
        while not self.stop.is_set():
            c = cfg()
            try:
                self.scan()
                self.dispatch(c)
                self.cleanup(c)
            except Exception as e:
                self.log(f"loop error: {e!r}")
            if now() - beat > 1:
                beat = now()
                with self.lock:
                    n = sum(1 for a in self.active.values() if a["phase"] == "kani")
                    hosts = [{"name": x["name"], "slots": x["slots"], "state": x["state"], "note": x["note"],
                              "ssh": x["ssh"], "memory_max": x["memory_max"], "checked": x.get("checked"),
                              "running": sum(1 for a in self.active.values() if a["phase"] == "kani" and a["host"] == x["name"]),
                              "peak_running": self.peaks.get(x["name"], 0)} for x in self.host_table(c)]
                write_json(HEARTBEAT, {"pid": os.getpid(), "ts": beat, "workers": c["workers"], "kani_running": n,
                                       "peak_kani_running": self.peak, "paused": self.paused, "hosts": hosts})
            self.stop.wait(0.3)
        self.log("stopping: killing running jobs and putting them back in the queue")
        with self.lock:
            threads = [a["thread"] for a in self.active.values()]
        for t in threads:
            t.join(15)
        HEARTBEAT.unlink(missing_ok=True)


# ---------------------------------------------------------------- CLI

def daemon_alive():
    hb = read_json(HEARTBEAT)
    return hb if hb and now() - hb["ts"] < 10 else None


def cmd_submit(a):
    ensure_dirs()
    c = cfg()
    crate = Path(a.worktree).expanduser().resolve()
    if not (crate / "Cargo.toml").exists():
        # a worktree whose crate lives one level down (garagebot-rs: <worktree>/logic)
        subs = sorted(f.parent for f in crate.glob("*/Cargo.toml"))
        if len(subs) != 1:
            sys.exit(f"proofq: no Cargo.toml in {crate}"
                     + (f"; pass one of: {', '.join(map(str, subs))}" if subs else " or its subdirectories"))
        crate = subs[0]
    top, _ = toplevel(crate)
    timeout = a.timeout if a.timeout else c["default_timeout"]
    if timeout <= 0 or timeout > c["max_timeout"]:
        sys.exit(f"proofq: --timeout must be in 1..{c['max_timeout']}")
    harnesses = sorted(set(a.harness))
    z = sorted(set(c["default_z"]) | set(a.Z))
    job = {"crate": str(crate), "top": str(top), "harnesses": harnesses, "exact": not a.no_exact, "z": z,
           "unwind": a.unwind, "default_unwind": a.default_unwind, "solver": a.solver, "kani_args": a.kani_arg, "timeout": timeout,
           "priority": a.priority, "who": a.who or os.environ.get("PROOFQ_WHO") or top.name,
           "check": not a.no_check, "test": not a.no_test, "cex": not a.no_cex, "no_cache": a.no_cache}
    targets = contract_targets(crate)
    fns = sorted({targets[x.split("::")[-1]] for x in harnesses if x.split("::")[-1] in targets})
    job["function"] = a.function or ", ".join(fns) or (", ".join(harnesses) if harnesses else "(all harnesses)")
    job["seed_id"] = seed_id_for(crate, top)
    key = compute_key(job)
    hit = None if a.no_cache else read_json(RESULTS / f"{key}.json")
    with open(STATE / "seq", "a+") as lockf:
        fcntl.flock(lockf, fcntl.LOCK_EX)
        live = [j for j in all_jobs() if j["state"] != "done" and not (JOBS / f"{j['id']}.cancel").exists()]
        if not hit:
            same = [] if a.no_cache else [j for j in live if j.get("key") == key and j["top"] == str(top)]
            if same:
                print(f"proofq: identical to {same[0]['id']} ({same[0]['state']}); returning that job", file=sys.stderr)
                print(same[0]["id"])
                return
            mine = [j for j in live if j["who"] == job["who"] and j["state"] in WAITING
                    and (a.no_supersede or j["top"] != str(top))]
            if len(mine) >= c["max_queued_per_who"]:
                print(f"proofq: '{job['who']}' already has {len(mine)} queued jobs "
                      f"({', '.join(j['id'] for j in mine)}); limit is {c['max_queued_per_who']}. "
                      f"Wait for one or `proofq cancel <id>`.", file=sys.stderr)
                sys.exit(3)
        lockf.seek(0)
        seq = int(lockf.read().strip() or 0) + 1
        lockf.seek(0)
        lockf.truncate()
        lockf.write(str(seq))
        lockf.flush()
        job.update(id=f"j{seq:04d}", seq=seq, key=key, submitted_at=now(), state="queued")
        if hit:
            res = dict(hit)
            res.update(cached=True, cached_from=hit.get("job"), orig_wall_secs=hit.get("wall_secs"), wall_secs=0.0)
            job.update(state="done", finished_at=now(), result=res)
        elif not a.no_supersede:
            for j in live:
                if j["top"] == str(top) and j["state"] in WAITING:
                    write_json(JOBS / f"{j['id']}.cancel", {"reason": "superseded", "by": job["id"]})
                    print(f"proofq: {j['id']} (same worktree, still queued) superseded by {job['id']}", file=sys.stderr)
        write_json(JOBS / f"{job['id']}.json", job)
    if hit:
        print(f"proofq: cache hit ({res['status']}, proved by {res.get('cached_from')})", file=sys.stderr)
    elif not daemon_alive():
        print("proofq: warning: the daemon is not running (systemctl --user start proofq)", file=sys.stderr)
    print(job["id"])


def load_job(jid):
    j = read_json(JOBS / f"{jid}.json")
    if not j:
        print(f"proofq: no such job {jid}", file=sys.stderr)
        sys.exit(2)
    return j


def position(job, jobs):
    if job["state"] not in WAITING:
        return None
    waiting = sorted((j for j in jobs if j["state"] in WAITING), key=order_key)
    return [j["id"] for j in waiting].index(job["id"]) + 1


def summary(job, jobs=None):
    """The machine-readable view of a job (status --json rows, wait --json)."""
    r = job.get("result") or {}
    t = now()
    s = {"id": job["id"], "who": job["who"], "function": job.get("function"), "harnesses": job["harnesses"],
         "worktree": job["crate"], "priority": job["priority"], "state": job["state"],
         "status": r.get("status"), "cached": bool(r.get("cached")),
         "position": position(job, jobs) if jobs is not None else None,
         "submitted_at": job["submitted_at"], "started_at": job.get("started_at"),
         "finished_at": job.get("finished_at"), "timeout": job["timeout"],
         "host": r.get("host") or job.get("host"), "requeues": job.get("requeues", 0)}
    if job["state"] == "done":
        s["seconds"] = r.get("wall_secs")
    elif job["state"] == "running":
        s["seconds"] = round(t - job["started_at"], 1)
    else:
        s["seconds"] = round(t - job["submitted_at"], 1)
    s["kani_secs"] = r.get("kani_secs")
    return s


def full_result(job):
    s = summary(job)
    r = job.get("result") or {}
    s["exit_code"] = EXIT.get(r.get("status"), 2)
    for k in ("stage", "check_secs", "counterexample_secs", "peak_mb", "cached_from", "orig_wall_secs", "note", "requeue_note",
              "precheck_note", "counterexample_note", "superseded_by", "excerpt", "compile_errors"):
        if r.get(k) is not None:
            s[k] = r[k]
    s["key"] = job.get("key")
    s["harnesses"] = r.get("harnesses", [])
    s["log"] = f"proofq log {job['id']}"
    return s


def print_result(job):
    r = job["result"]
    st = r["status"].upper()
    kani = f" kani={r['kani_secs']}s" if r.get("kani_secs") is not None else ""
    print(f"{job['id']} {st}  wall={r.get('wall_secs')}s{kani} stage={r.get('stage')}"
          f"{'  (cached, proved by ' + str(r.get('cached_from')) + ')' if r.get('cached') else ''}"
          f"  who={job['who']} fn={job.get('function')}"
          f"{'  host=' + r['host'] if r.get('host') not in (None, LOCAL) else ''}")
    for k in ("note", "precheck_note", "counterexample_note", "requeue_note"):
        if r.get(k):
            print(f"  note: {r[k]}")
    if r.get("superseded_by"):
        print(f"  superseded by {r['superseded_by']}: run `proofq wait {r['superseded_by']}`")
    for x in r.get("harnesses", []):
        print(f"  {str(x['status']).upper():8} {x['name']}  {x['secs'] if x['secs'] is not None else '-'}s")
        for f in x.get("failures", []):
            print(f"      failed: {f['description']}")
            print(f"          at {f['file']}:{f['line']} in {f['function']}: {f['source'] or ''}")
        cx = x.get("counterexample")
        if cx:
            print("      counterexample (i-th kani::any() on the failing path): "
                  + ", ".join(f"#{v['index']}={v['value']}" for v in cx["values"]))
    if r.get("excerpt"):
        print("  ---")
        for ln in r["excerpt"].splitlines():
            print("  | " + ln)
    print(f"  full output: proofq log {job['id']}")


def cmd_wait(a):
    warned = False
    while True:
        job = load_job(a.id)
        if job["state"] == "done":
            break
        if not warned and not daemon_alive():
            print("proofq: warning: the daemon is not running; this will block until it is started "
                  "(systemctl --user start proofq)", file=sys.stderr)
            warned = True
        time.sleep(0.4)
    if a.json:
        print(json.dumps(full_result(job), indent=1))
    else:
        print_result(job)
    sys.exit(EXIT.get(job["result"]["status"], 2))


def cmd_status(a):
    ensure_dirs()
    jobs = all_jobs()
    hb = daemon_alive()
    c = cfg()
    if a.id:
        job = load_job(a.id)
        if a.json:
            print(json.dumps(full_result(job) if job["state"] == "done" else summary(job, jobs), indent=1))
        elif job["state"] == "done":
            print_result(job)
        else:
            s = summary(job, jobs)
            pos = f" position {s['position']} in queue," if s["position"] else ""
            print(f"{job['id']} {job['state']},{pos} {s['seconds']}s so far, who={job['who']} fn={job.get('function')}")
        return
    rows = [summary(j, jobs) for j in jobs]
    hosts = (hb or {}).get("hosts")
    if not hosts:   # daemon down (or an older one): what the config says, state unknown
        hosts = [{"name": LOCAL, "slots": c["workers"], "state": "up" if hb else "?", "note": "", "ssh": None,
                  "memory_max": c["memory_max"], "running": 0}]
        hosts += [{"name": w["name"], "slots": w["slots"], "state": "?" if w["enabled"] and w["slots"] else "disabled",
                   "note": "", "ssh": w["ssh"], "memory_max": w["memory_max"] or c["worker_memory_max"], "running": 0}
                  for w in workers_cfg()]
    total = sum(x["slots"] for x in hosts if x["state"] in ("up", "?"))
    live = [r for r in rows if r["state"] != "done"]
    recent = sorted((r for r in rows if r["state"] == "done"), key=lambda r: r["finished_at"])[-a.recent:]
    if a.json:
        print(json.dumps({"now": now(),
                          "daemon": {"alive": bool(hb), "workers": c["workers"], "slots": total, "hosts": hosts,
                                     "kani_running": sum(r["state"] == "running" for r in live),
                                     "peak_kani_running": (hb or {}).get("peak_kani_running"),
                                     "paused": (hb or {}).get("paused")},
                          "counts": {s: sum(r["state"] == s for r in rows) for s in (*WAITING, "running", "done")},
                          "jobs": live + recent}, indent=1))
        return
    run = [r for r in live if r["state"] == "running"]
    print(f"daemon: {'up' if hb else 'NOT RUNNING'}   kani slots: {len(run)}/{total}   "
          f"waiting: {len(live) - len(run)}" + (f"   PAUSED: {hb['paused']}" if hb and hb.get("paused") else ""))
    if len(hosts) > 1:
        print("hosts (slots in use/total):")
        for x in hosts:
            use = "-" if x["state"] == "disabled" else f"{x['running']}/{x['slots']}"
            st = {"down": "DOWN"}.get(x["state"], x["state"])
            ago = f" (checked {now() - x['checked']:.0f}s ago)" if x["state"] == "down" and x.get("checked") else ""
            print(f"  {x['name'][:20]:20} {use:>5}  {st:9} MemoryMax={x['memory_max']:4} {x['ssh'] or 'this host':22} {x['note']}{ago}".rstrip())

    def line(r, extra):
        at = f"  @{r['host']}" if r.get("host") not in (None, LOCAL) else ""
        print(f"  {r['id']}  {extra:14} {r['seconds']:>7}s  {r['who'][:18]:18} {(r['function'] or '')[:40]}{at}")
    if run:
        print("running:")
        for r in run:
            line(r, "running" + (" [high]" if r["priority"] == "high" else ""))
    wait = sorted((r for r in live if r["state"] != "running"), key=lambda r: r["position"])
    if wait:
        print("waiting (position, state):")
        for r in wait:
            line(r, f"#{r['position']} {r['state']}" + (" [high]" if r["priority"] == "high" else ""))
    if recent:
        print(f"last {len(recent)} finished:")
        for r in recent:
            line(r, r["status"] + (" (cached)" if r["cached"] else ""))


def cmd_log(a):
    job = load_job(a.id)
    r = job.get("result") or {}
    p = LOGS / f"{a.id}.log"
    if r.get("cached") and not p.exists():
        p = RESULTS / f"{job['key']}.log"
    try:
        sys.stdout.write(p.read_text(errors="replace"))
    except FileNotFoundError:
        print(f"proofq: no log yet for {a.id} (state {job['state']})", file=sys.stderr)
        sys.exit(2)


def cmd_cancel(a):
    job = load_job(a.id)
    if job["state"] == "done":
        print(f"{a.id} already finished ({job['result']['status']})")
        return
    write_json(JOBS / f"{a.id}.cancel", {"reason": "cancelled"})
    print(f"{a.id} cancel requested (was {job['state']})")


def main():
    p = argparse.ArgumentParser(prog="proofq", description="Job queue for cargo kani runs.")
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("submit", help="queue a Kani run; prints the job id")
    s.add_argument("worktree", help="your git worktree (or the crate directory inside it, if it holds several crates)")
    s.add_argument("--harness", action="append", default=[], metavar="NAME",
                   help="harness to run, repeatable (exact, fully qualified name); default: all")
    s.add_argument("--timeout", type=int, metavar="S", help="Kani seconds for this job")
    s.add_argument("--priority", choices=["normal", "high"], default="normal")
    s.add_argument("--who", help="submitter name (default: $PROOFQ_WHO, else the worktree directory name)")
    s.add_argument("--function", help="label: the function under proof (default: read from proof_for_contract)")
    s.add_argument("--unwind", type=int, metavar="N", help="passed to cargo kani --unwind (needs --harness)")
    s.add_argument("--default-unwind", type=int, metavar="N", help="passed to cargo kani --default-unwind")
    s.add_argument("--solver", metavar="NAME", help="passed to cargo kani --solver (cadical, kissat, minisat, z3, bitwuzla, cvc5, bin=<path>)")
    s.add_argument("-Z", action="append", default=[], metavar="FEATURE",
                   help="extra unstable Kani feature, repeatable (function-contracts and stubbing are always on)")
    s.add_argument("--kani-arg", action="append", default=[], metavar="ARG", help="any other cargo kani argument (use --kani-arg=--flag)")
    s.add_argument("--no-exact", action="store_true", help="match --harness as a substring")
    s.add_argument("--no-check", action="store_true", help="skip the cargo check/test pre-check")
    s.add_argument("--no-test", action="store_true", help="skip cargo test in the pre-check")
    s.add_argument("--no-cex", action="store_true", help="skip the counterexample pass on failure")
    s.add_argument("--no-cache", action="store_true",
                   help="ignore the result cache (no hit at submit, none from an identical running job); for final full runs")
    s.add_argument("--no-supersede", action="store_true", help="keep this worktree's older queued job")
    s.set_defaults(fn=cmd_submit)
    s = sub.add_parser("wait", help="block until the job is done; exit 0 pass / 1 fail / 2 error, timeout, cancelled")
    s.add_argument("id")
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_wait)
    s = sub.add_parser("status", help="queue overview, or one job")
    s.add_argument("id", nargs="?")
    s.add_argument("--json", action="store_true")
    s.add_argument("--recent", type=int, default=10, help="finished jobs to show")
    s.set_defaults(fn=cmd_status)
    s = sub.add_parser("log", help="full cargo/Kani output of a job")
    s.add_argument("id")
    s.set_defaults(fn=cmd_log)
    s = sub.add_parser("cancel", help="cancel a queued or running job")
    s.add_argument("id")
    s.set_defaults(fn=cmd_cancel)
    s = sub.add_parser("daemon", help="run the worker pool (proofq.service)")
    s.set_defaults(fn=lambda a: Daemon().run())
    a = p.parse_args()
    try:
        a.fn(a)
    except BrokenPipeError:
        pass


if __name__ == "__main__":
    main()

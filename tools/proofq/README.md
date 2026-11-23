# proofq

A job queue for `cargo kani` runs on eE (8 cores, 31 GB). Agents submit from their own
git worktree; a systemd user service runs at most 6 Kani jobs at a time on eE, each in its own
`systemd-run --user --scope -p MemoryMax=4G` with a timeout. Extra machines can add slots:
see [Remote workers](#remote-workers).

Files: `proofq.py` (CLI and daemon, stdlib only, run with `uv run --script`), `proofq.sh`
(launcher, symlinked as `~/.local/bin/proofq`), `proofq.service`, `e2e.sh` (end-to-end test),
`e2e-workers.sh` (remote-worker test). The worker list is `~/.config/proofq/workers.json`.

## Agent commands

```sh
export PATH="$HOME/.local/bin:$PATH"          # if proofq is not found

# submit: prints the job id on stdout (messages go to stderr)
id=$(proofq submit ~/worktrees/agent-17 --who agent-17 --harness contract_decide)

# block until done. Exit code: 0 pass, 1 fail, 2 error / timeout / cancelled / superseded
proofq wait "$id" --json

proofq status            # queue overview; `proofq status <id>` for one job; --json for both
proofq log "$id"         # full cargo and Kani output
proofq cancel "$id"      # queued or running
```

`submit` options:

| option | meaning |
|---|---|
| `<worktree>` | the worktree root. If it has no `Cargo.toml` but exactly one subdirectory does (garagebot-rs: `logic/`), that crate is used. Otherwise pass the crate directory. |
| `--harness N` | repeatable, exact harness name. Default: all harnesses. `--no-exact` for substring match. |
| `--timeout S` | Kani seconds for this job. Default 300, max 3600. |
| `--who NAME` | submitter. Default `$PROOFQ_WHO`, else the worktree directory name. |
| `--priority high` | runs before all normal jobs. |
| `--unwind N`, `--default-unwind N`, `--solver NAME`, `-Z FEATURE` | passed to `cargo kani`. `-Z function-contracts -Z stubbing` are always on. |
| `--kani-arg=--flag` | any other `cargo kani` argument. |
| `--no-cache` | ignore the result cache: no hit at submit, no waiting for an identical running job. Use it for final full runs. The fresh result is still stored. |
| `--no-check`, `--no-cex`, `--no-supersede` | skip the `cargo check` pre-check, skip the counterexample pass, keep this worktree's older queued job. |

### `wait --json`

Top level: `id`, `status` (`pass`, `fail`, `timeout`, `error`, `cancelled`, `superseded`),
`exit_code`, `stage` (`check`, `kani`, `kani-compile`, `queue`), `cached`, `cached_from`,
`kani_secs`, `host` (`local` or the worker that gave the verdict), `requeues` and
`requeue_note` (the job was started on a worker and put back in the queue), `note`, `excerpt`,
`compile_errors` (for a failed `cargo check`: `message`,
`code`, `file`, `line`, `column`, `rendered`), `superseded_by`, and `harnesses`:

```json
{"name": "contract_ticks_add", "status": "fail", "secs": 0.35,
 "failures": [{"check": "...", "status": "FAILURE",
   "description": "|r| *r < TICKS_PERIOD && ...",       // the failed assertion
   "file": "src/lib.rs", "line": 48, "column": 18, "function": "...",
   "source": "#[cfg_attr(kani, kani::ensures(|r| *r < TICKS_PERIOD"}],
 "counterexample": {"values": [{"index": 0, "value": "0", "bytes": [0,0,0,0]},
                               {"index": 1, "value": "536870912", "bytes": [0,0,0,32]}],
                    "playback_test": "..."}}
```

`counterexample.values[i]` is the i-th `kani::any()` the harness evaluates on the failing
path. It comes from a second Kani pass with concrete playback, which takes about 100 s
for a `proof_for_contract` harness (CBMC trace generation) and is cut off at 300 s or the
job's `--timeout`, whichever is lower; `counterexample_note` says so and the result is then
not cached. `--no-cex` skips it. A harness whose
`kani::cover!` is unsatisfied is reported as `fail` (vacuous harness).

## Queue rules

- 6 Kani jobs at once on eE (`workers`), plus the slots of every remote worker that is up.
  eE's slots fill first. One job per worktree at a time.
- `cargo check` runs first, outside the Kani slots (2 at a time, 3 GB). A compile error
  ends the job as `fail`, `stage: check`, with `compile_errors`; it never takes a Kani slot.
- At most 2 waiting jobs per `--who`; a third `submit` exits 3 and prints the waiting ids.
- A new submit from a worktree supersedes that worktree's older waiting job (the old job
  ends as `superseded`, exit 2, with `superseded_by`). A running job is not interrupted.
- Result cache, key per harness. A job's key is built from the key of each of its `--harness`
  names (exact names only) plus the Kani version, the flags (`-Z`, unwind, solver, `--kani-arg`)
  and the crate path. A harness key (same marker scheme as `garagebot-rs/cache.py`:
  `// @prelude`, `// @fn`/`// @body`, `// @harness NAME target= stubs= uses=`, `// @end`) is the hash of:
  the prelude; the harness source and its header; the full text of its target; the CONTRACT
  (lines above `// @body`) of every function it stubs (header `stubs=` plus any `kani::stub(f, ..)`
  in its source); the full text of every region function whose name appears as a word in what the
  harness runs and is not stubbed (transitively, plus the declared `uses=`); all of `lib.rs` and
  `spec.rs`; the lines of every other `.rs` file outside regions; and every other file of the crate
  directory (`Cargo.toml`, `Cargo.lock`, `build.rs`, toolchain files, `.cargo/`, ...) with the
  top-level manifests. `*.md`, `LOG.md`, `ROOT.txt` and `.proofcache.json` are left out. So a body
  edit of a function the harness stubs, or of an unrelated function, keeps its cached result;
  a contract edit, or an edit of anything it runs, does not. If the layout cannot be parsed
  (no markers, a harness name not found, duplicate region names, `[workspace]`, a path dependency
  or `include!` outside the crate, `--no-exact`, no `--harness`) the key falls back to the old rule:
  sha256 of every `git ls-files` file of the whole worktree. Only `pass` and `fail` are cached;
  timeouts and errors are not. Kept 30 days. A multi-harness job hits only if every harness key matches.
- Learned routing. The daemon records, per harness name, the run time and the peak memory of the
  Kani scope (cgroup `memory.peak`, or sampled `memory.current` on the Jetsons) in
  `~/.local/state/proofq/history.json`. A harness whose peak is below `route_small_peak_gb`
  (4.5) goes to a remote worker whose `memory_max` fits it; a bigger one, and one that a worker
  killed or timed out (`needs_local`, kept), waits for a local slot. A small one spills to a local
  slot only if that leaves a slot for each waiting big one. A harness with no history takes the
  first free slot, this host first. A local job whose recorded peak times 1.25 exceeds `memory_max`
  gets that much memory, up to `big_memory_max_gb` (8). The waiting queue is ordered by priority,
  then longest expected run first (a harness never seen counts as longest, so it is measured early),
  then submission order. `wait --json` has `peak_mb`. Delete `history.json` to forget what was learned.
- Timeout: the scope is killed, status `timeout`, exit 2. Over 4 GB: status `error`,
  note "out of memory".
- A daemon restart kills running jobs and puts them back in the queue.

## Remote workers

Other machines can run Kani jobs for the queue. The daemon on eE copies the job's worktree
to the worker with rsync (the same files the cache key covers: `git ls-files` plus
`Cargo.lock`), runs `cargo check` and then the same `cargo kani` command there over ssh, in a
`systemd-run --user --scope -p MemoryMax=...` with the job's timeout, and streams the output
into the job's log. The result has the same JSON as a local run, plus `"host"`. Submitting,
the cache, the per-`--who` limit, supersede and priorities work as before; agents do not
choose a host.

The list is `~/.config/proofq/workers.json`, read live (no restart):

```json
{"workers": [
  {"name": "worker-b", "ssh": "user@<worker-b-ip>",  "slots": 1, "memory_max": "3G", "enabled": true},
  {"name": "worker-a",  "ssh": "user@<worker-a-lan-ip>", "ssh_opts": ["-o", "HostKeyAlias=<worker-a-ip>"],
   "slots": 1, "memory_max": "3G", "enabled": true}
]}
```

`slots` is the number of Kani jobs the worker runs at once, `memory_max` the limit for each
(default `worker_memory_max`, 3G). Optional `ssh_opts`: a list of extra `ssh` arguments
(worker-a is reached on its LAN address; `HostKeyAlias` checks it against the host key
already known under its Tailscale address).

**Add a worker**

1. From eE, `ssh -o BatchMode=yes <user>@<host> true` must work with no prompt: the worker's
   host key in eE's `~/.ssh/known_hosts` and a key of eE's in the worker's `authorized_keys`.
   The worker needs `rsync`, cgroup v2 and `loginctl enable-linger <user>`.
2. On the worker, install the same Rust and Kani as eE (`cargo kani --version` must print the
   same first line, or the worker is refused):
   `rustup` with toolchain 1.100.0, then `cargo install --locked kani-verifier --version 0.68.0 && cargo kani setup`.
   On Ubuntu 22.04 / JetPack 6 (glibc 2.35) the aarch64 bundle's `kani-driver` does not start
   (it needs glibc 2.39): rebuild it from the `kani-0.68.0` tag, as
   `~/.cache/proofq-worker/build-kani-driver.sh` on worker-b does, or copy `~/.kani` from
   a Jetson that has it (same JetPack and glibc; worker-a's was copied from worker-b).
   If the worker's default Rust is another version, leave it and run
   `rustup override set 1.100.0 --path ~/.cache/proofq-worker` (worker-a has this).
3. Pick `slots` from the worker's free memory: (`MemAvailable` − 2 GB for its own services) / `memory_max`.
   Measured 2026-11-22 with their LLM servers stopped (lfm-router on worker-a, lfm-omni on
   worker-b): 5.9 GB and 5.8 GB available, so 1 slot at 3G each. When a 3.5 GB model is
   loaded again there is no room for a 3G slot: disable the worker first. A 1200M limit is
   too small for the larger harnesses (`poll_step`) and for the counterexample pass.
4. Add the entry to `workers.json` with `"enabled": true`. Within `worker_recheck_secs` (30 s)
   `proofq status` shows it `up`, or `DOWN` with the reason.

**Disable a worker**: set `"enabled": false` (or `"slots": 0`) in `workers.json`. No new job
goes there; a job already running on it finishes. To stop that one too, `proofq cancel <id>`.

`proofq status` lists the slots in use and the total for every host:

```
daemon: up   kani slots: 7/7   waiting: 3
hosts (slots in use/total):
  local                  6/6  up        MemoryMax=4G   this host
  worker-b          1/1  up        MemoryMax=3G   user@<worker-b-ip>
  worker-a           0/1  DOWN      MemoryMax=3G   user@<worker-a-ip>      ssh: connect to host <worker-a-ip> port 22: Connection timed out (checked 12s ago)
running:
  j0212  running           31.0s  agent-17           decide  @worker-b
```

`proofq status --json` has the same under `daemon.hosts` (`name`, `slots`, `running`, `state`:
`up`, `down` or `disabled`, `note`, `peak_running`), and `daemon.slots` is the total of the hosts that are up.

Rules:

- A worker is `up` only if ssh answers, its Kani version equals eE's and it has
  `worker_min_free_gb` (10) of free disk. A down worker is probed again every 30 s and comes
  back by itself.
- If the connection to a worker is lost during a job (ssh keepalive: about 15 s), the worker
  is marked down and the job goes back to the queue at its old position. Whatever was left
  running on the worker is killed when it answers again, or by the scope's own time limit.
- Only a Kani verdict (`pass` or `fail`) is taken from a worker. A timeout, an out-of-memory
  kill or a build error there (also in the counterexample pass) puts the job back in the queue for a local run on eE, so these
  mean the same as before (the workers are slower and have less memory). The result then has
  `"requeues": 1` and a `requeue_note`.
- A proof that needs more memory than the worker's limit costs its run time there before it
  starts again on eE (2026-11-22: several garagebot `contract_*` harnesses use 3.5 GB or more
  in CBMC and were killed on the Jetsons at 3G and at 5G). `journalctl --user -u proofq | grep requeue`
  shows how often; if most remote jobs end that way, disable the worker.
- On the worker everything is under `~/.cache/proofq-worker/` (`src/<worktree>`,
  `target/<worktree>`); copies idle for 24 h are deleted. Dependencies are built once per
  worktree there (no shared seed as on eE).
- Do not point two proofq daemons at the same worker: each kills the other's leftover scopes.

`~/proofq/e2e-workers.sh` tests this against the live queue: 12 jobs at once and the spread
over the hosts, then a worker cut off in the middle of a job.

## Build sharing and disk

Each worktree gets its own `CARGO_TARGET_DIR` under `~/.cache/proofq/target/` (the
worktree's own `target/` is not used). The first job with a given `Cargo.lock` builds the
registry and git dependencies once; that build is kept as a `_seed-*` directory and copied
into every other worktree's target dir with the same `Cargo.lock`, so only local crates are
rebuilt. Target dirs idle for 24 h are deleted, and the least recently used ones go first
when the total exceeds 15 GB. No job starts while the disk has less than 20 GB free.

## Configuration

Optional `~/.config/proofq/config.json`, read live. Keys and defaults are in `DEFAULTS` at
the top of `proofq.py`, for example `{"workers": 4, "memory_max": "4G", "default_timeout": 600,
"max_queued_per_who": 2, "precheck_test": true}`. Remote hosts are in `workers.json` next to it
(see [Remote workers](#remote-workers)).

State: `~/.local/state/proofq/` (jobs, logs, result cache). Daemon log:
`journalctl --user -u proofq -f`.

## Service

Installed as `~/.config/systemd/user/proofq.service` (a copy of `proofq.service`), enabled,
`Restart=on-failure`; lingering is on for `mat`, so it starts at boot.

```sh
systemctl --user status proofq
systemctl --user restart proofq                 # after editing proofq.py
```

Disable it:

```sh
systemctl --user disable --now proofq           # stop now and do not start at boot
```

Queued jobs stay on disk and resume after `systemctl --user enable --now proofq`.
Remove everything: disable as above, then
`rm -rf ~/.config/systemd/user/proofq.service ~/.local/bin/proofq ~/.local/state/proofq ~/.cache/proofq ~/proofq ~/proofq-test`
and `systemctl --user daemon-reload`.

## Test

`~/proofq/e2e.sh` (about 4 minutes; it stops and starts the service) builds 10 git worktrees
of a copy of `~/garagebot-kani-loop` in `~/proofq-test/e2e`, submits 10 jobs at once and
prints the checks. The last run's output is in `e2e-last.txt`.

# Formal verification of the GarageBot Rust rewrite

GarageBot is being rewritten from MicroPython (`main.py`) to Rust. The decision logic of the
Rust version is proven with [Kani](https://model-checking.github.io/kani/), a model checker
that checks a property for every possible input, not for a set of examples. This page says what
was proven, how the work was organised, what came out, and what is still open.

Status as of the last recorded proof run (2026-11-23, `firmware-rs/LOG.md`): **150 of 154
proofs pass.** The 4 that fail are checks written by the verifier that turned out to be too
strict; see "Still open".

## What was verified

The firmware is split in two crates.

- `firmware-rs/logic/` holds every decision the bot makes: who may press the relay, when a
  command is too old, when to alert, retry, reboot or install an update, and all parsing and
  message text. It is pure code with no hardware or network access. This part is proven.
- `firmware-rs/shell/` is the ESP32 code around it: one loop that feeds the watchdog, asks the
  logic for the next operation and carries it out (WiFi, HTTPS, GPIO, flash). This part is read
  by a person, not proven. `firmware-rs/TRUSTED.md` lists it, together with every other
  assumption the proofs rest on.

There are three layers of proof.

1. **Contracts (73).** Each logic function has a specification written as plain executable
   code in `spec.rs`. A contract proof shows that the function returns the same result as its
   specification for every input, and never panics.
2. **Properties and lemmas (33).** These are the promises that matter to a user, proven from
   the specifications. Examples: a stale command never presses the relay, an unauthorised chat
   never acts, a command is never run twice, a failed poll loses nothing, a stuck WiFi connect
   cannot block the loop, an update is installed only if it is newer and the image checks out.
3. **Verifier checks (48).** A second set of properties written independently; see below.

## The approach

The work was done by AI agents in separate roles, with Kani as the judge.

1. **Architect.** One agent wrote `CONTRACTS.md`, the specifications and the function stubs,
   and proved the top-level properties from the contracts alone, before any function body
   existed. It then split the heavy functions until each proof fitted in time and memory.
2. **Blind verifier.** A second agent wrote `VERIFIER_SPEC.md` and
   `logic/src/verifier_proofs.rs` from what the bot is for, without reading the architect's
   contracts or any body. It also challenged `TRUSTED.md`. The point is that the same mistake
   is unlikely to be made twice by two agents that did not see each other's work.
3. **Builder fan-out.** Many small agents each wrote the body of one function in their own git
   worktree and iterated against Kani until the contract proof passed. Callees stayed stubbed
   to their contracts, so each builder only needed its own function. The plan was to run the
   same functions on Haiku, Sonnet, Opus and Fable as a benchmark.
4. **Finisher.** One pass merged the bodies, proved everything again with no skips, and wrote
   the Merkle root of all sound proofs to `ROOT.txt`. The firmware shows the first 8 hex digits
   of that root in its `/version` reply.

All Kani runs went through **proofq** (`tools/proofq/`), a small job queue. It runs at most 6
Kani jobs at once on the build desktop, each with a 4 GB memory limit and a timeout, and caches
results by source hash so an unchanged proof is not run twice. Two small remote workers could
take light jobs.

Two earlier, smaller runs led to this design and are kept in `firmware-rs/history/`: a batch
run (write everything, then prove it all at once) and an in-loop run (prove each function as it
is written, with a result cache).

## Results

- **Cache-tracked proofs: 106 of 106 pass and are sound** (73 contracts, 33 properties and
  lemmas). Root `0c87ab77…`, in `ROOT.txt`.
- **Verifier checks: 44 of 48 pass.**
- **Per-model benchmark: only Haiku was run.** Fleet 1 was 125 Haiku builders, 5 for each of
  25 functions. 120 passed, 119 of them on the first proof run, in about 56 minutes. All 5
  failures were one function (`on_update`), where the proof ran out of memory before giving a
  verdict; that was a fault in the proof setup, not in the model. On leaf functions Haiku
  passed 95 of 95. The Sonnet, Opus and Fable runs and the second Haiku fleet were not
  launched, so there is no cross-model table yet. `EXPERIMENT.md` has the per-builder rows.
- **Where the bodies came from.** At the finisher pass there were 69 functions: 22 bodies are
  from Haiku builders and 47 were rendered from the specification text by a script
  (`tools/fill_bodies.py`). The OTA functions were added after that.
- **Earlier runs.** The batch run verified 20 of 27 harnesses and the in-loop run 26 of 26.
  Between them they found the bugs in the Python version that are listed below.

## Lessons

The lessons are in [FV_LESSONS.md](../FV_LESSONS.md). The short version:

- When the specification is executable code and the contract is "result equals the
  specification", writing the body is trivial, so it is not a benchmark of the model. The hard
  thinking is in the specification and the contracts.
- A proof that runs out of memory looks like a model failure but is a harness problem. Prove
  each harness once with a reference body before fanning out, and try one builder per function
  before many.
- The bug a user sees can sit in the trusted part. The old bot stopped answering because the
  TLS handshake ran out of memory, which no logic proof would have found.

## Still open

- **4 verifier checks fail** (`v1_reply_within_two_passes`, `v1_poll_due_after_any_pause`,
  `v5_open_batch_makes_progress`, `v6_keeps_polling_when_polls_fail`). They share one cause.
  The specification reboots the bot when no poll has succeeded for a long time, and these four
  checks did not allow for that. The fix is in the checks, not in the logic: let them accept
  "or a deaf-reboot happened", and add a check that such a reboot only happens after the full
  timeout. That would give 154 of 154, plus the new check.
- **The 6 known bugs are still in the Python version** (`main.py`); the Rust design fixes
  them. They are: a WiFi connect that can hang forever; a flush timeout that drops a fresh
  command; a stale `/open` that runs before the clock is synced (the worst one); negative
  minutes in the open-door alert after 7.57 days; uptime that drifts; and a fresh command behind
  `/update` that is dropped.
- **Flashing.** [FV_LESSONS.md](../FV_LESSONS.md) lesson 7 records a first USB flash of the
  Rust firmware and a clean boot. Nothing in this repo flashes a board by itself:
  `build_and_flash.sh` only prints the commands. No eFuse is burned and flash is not encrypted.
- The shell (434 lines) and the rest of `TRUSTED.md` are read, not proven. Slice-taking
  functions are proven only up to the length bounds listed there.
- `TRUSTED.md` and `CONTRACTS.md` still carry a few sentences from before the bodies existed.

## File map

`firmware-rs/`

| Path | What it is |
|---|---|
| `CONTRACTS.md` | The architect's document: every function, its contract and callees, the top-level properties, and how the proof is put together. |
| `VERIFIER_SPEC.md` | The blind verifier's own specification, where it found the contracts weaker, and its results. |
| `TRUSTED.md` | Everything the proofs rest on but do not prove, each with the assumption made and a fence that limits the damage. |
| `EXPERIMENT.md` | The builder experiment: one row per Haiku builder, per-function summary, audit, and the finisher's body-source table. |
| `FUNCTIONS.json` | Machine-readable list of the functions, signatures and callees, used to hand out builder tasks. |
| `LOG.md` | One line per proof run, with result, time and cache key. Remote worker names are replaced with `worker-a` and `worker-b`. |
| `ROOT.txt` | The Merkle root over all sound proofs, and the leaf hash of each one. |
| `cache.py` | Finds the harnesses, hashes what each one depends on, runs only the changed ones through proofq, and writes `LOG.md` and `ROOT.txt`. |
| `logic/Cargo.toml`, `Cargo.lock`, `build.rs` | The logic crate and its Kani settings. `build.rs` generates the module list from `src/fns/`. |
| `logic/src/lib.rs` | Types and constants shared by the logic. |
| `logic/src/spec.rs` | The specifications, as executable code. |
| `logic/src/props.rs` | The top-level properties and lemmas. |
| `logic/src/verifier_proofs.rs` | The 48 verifier checks. |
| `logic/src/fns/*.rs` | One file per function: the body and its contract harness. |
| `logic/src/fns_list.rs` | The list of function modules. |
| `tools/gen_stubs.py` | Generates the function stubs and `FUNCTIONS.json` from one table. |
| `tools/fill_bodies.py` | Renders a specification into a function body. |
| `tools/prove_all.py`, `import_results.py`, `finisher_table.py` | Submit every harness to proofq, import the results into the cache, and build the finisher table. |
| `shell/src/main.rs` | The ESP32 firmware shell (trusted, not proven). |
| `shell/README.md` | Flash layout, secrets in NVS, build steps and over-the-air updates. |
| `shell/Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`, `.cargo/config.toml` | The firmware crate and its ESP32 toolchain settings. |
| `shell/sdkconfig.defaults`, `partitions.csv` | ESP-IDF settings and the 4 MB flash layout with two app slots. |
| `shell/build_and_flash.sh` | Runs the build on the build host, checks the image holds no secrets, makes the secrets image locally and prints the flash commands. It reads `secrets.py`, which is not in git. |
| `shell/remote_build.sh` | The build and signing step that runs on the build host. |
| `history/kani-batch/` | The first experiment: a small port of the timing logic, a spec, 27 harnesses and the Kani log. |
| `history/kani-loop/` | The second experiment: the same logic proven function by function, with the first `cache.py`, its log and root. |

`tools/proofq/`

| Path | What it is |
|---|---|
| `proofq.py` | The queue: command-line tool and daemon in one file, standard library only. |
| `proofq.sh`, `proofq.service` | The launcher and the systemd user service. |
| `README.md` | How to submit and wait for jobs, the limits, the cache, and remote workers. Addresses are placeholders. |
| `e2e.sh`, `e2e-workers.sh` | End-to-end tests of the queue and of the remote workers. |

Not in git: build output (`target/`, `shell/out/`, release binaries), the ESP-IDF download, the
proof result cache, the builders' worktrees, `secrets.py` and the signing key.

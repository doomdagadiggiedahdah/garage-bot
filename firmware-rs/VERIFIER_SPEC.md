# GarageBot verifier spec

Written by the VERIFIER before reading CONTRACTS.md. Sources: `garage-bot/main.py` v1.6.0 (git history, commit `2341d02`),
`ARCHITECTURE.md`, and the earlier `garagebot-kani/SPEC.md`. It is derived from what the bot is
FOR, not from what main.py or the Rust port happens to do.

Purpose: a garage door Telegram bot. Two duties, in priority order:

1. **Safety**: the door relay is never pressed on an old or untrusted command.
2. **Liveness**: the bot always answers, whatever the network does.

Where the two conflict, safety wins. (Changed 2026-11-22 at the owner's request: a refused guarded
command is dropped silently, not answered. See V1.2.)

## Model

- Ticks: `ticks_ms` in `[0, 2^30)`, wrapping (period 2^30 ms, about 12.43 days). A single
  `ticks_diff` is only truthful for true elapsed time in `[0, 2^29)`.
- Network: every `getUpdates`, flush, `sendMessage` and NTP call ends in one of
  ok / timeout / error, chosen by an adversary. Timeouts are bounded (10 s HTTP, 30 s OTA).
- An update carries: `update_id`, chat id, text, optional `date` (Unix s).
- Clock: `synced` is false until one NTP call has succeeded since boot.
- "Guarded" commands are the ones that can press the relay or replace firmware:
  open, close, update.
- "Fresh" means: clock synced AND `date` present AND `0 <= now - date <= 60`.
  Anything else is stale (age > 60) or unverifiable (no sync, no date, date in the future).

## Properties

### V1 Always responds
For any bounded sequence of loop passes with any mix of ok/timeout/error:

- V1.1 A poll that fails changes nothing: offset not advanced, no command lost. The same
  updates are processed by the next successful poll.
- V1.2 Every recognised command from the authorised chat that a successful poll delivers and
  the bot acts on gets at least one reply attempt in that pass: /status and /debug at any age,
  a guarded command only when it is fresh. A guarded command that is stale, pre-boot or
  unverifiable is dropped silently: consumed (offset moves past it), never executed, no reply.
  (Until 2026-11-22 the user was told it was refused and why; the owner: "just silently drop it".)
- V1.3 No command is processed twice (offset moves strictly past it), so no double press.
- V1.4 A failed `sendMessage` never blocks later commands, the door monitor or the watchdog
  feed. The next pass runs normally.
- V1.5 Stronger, wanted: a reply whose send failed is kept and retried until one send
  succeeds (bounded queue), so "answers" means the user sees it once the network is back.
  main.py does not do this (one attempt, result ignored).
- V1.6 Polling is due again after ANY pause, including pauses longer than 2^29 ms (WiFi down
  for a week). The poll-due decision must not go silent because of tick aliasing.
- V1.7 Unknown text and other chats: no relay press, no reply.

### V2 Stale or unverifiable /open never actuates
The relay is pressed by a Telegram command only if ALL hold:

- from the authorised chat,
- `update_id` greater than every id processed before,
- the command is fresh as defined above.

So, for every clock state, date, and network history:

- V2.1 age > 60 s: no press.
- V2.2 clock not synced (before the first NTP success, or NTP down): no press. This is the
  main.py hole: `stale_command_age` returns None when `clock_synced` is false, so an `/open`
  queued during an outage is executed late.
- V2.3 `date` missing or zero: no press.
- V2.4 `date` in the future beyond a small skew: no press (our clock or theirs is wrong, so
  age is unverifiable).
- V2.5 Age arithmetic never overflows or panics for any `now`, `date` (full integer range).
- V2.6 Safety must not rest on the boot flush having succeeded. The flush is a convenience;
  the age check is the guard.
- V2.7 `/open` with the door already open, `/close` with it already closed: no press.

### V3 Close alert after the timeout, across wraparound
After a bot-triggered `/close` at tick `t0`, on each pass where the door still reads open:

- V3.1 alert iff true elapsed >= 60 000 ms, for every `t0` in `[0, 2^30)`, including when the
  counter wraps between `t0` and now.
- V3.2 at most once per close request; a new `/close` re-arms.
- V3.3 never if the door was seen closed first; never without a bot-triggered close.
- V3.4 still fires after a long pause (true elapsed in `[2^29, 2^30)` and beyond), where a
  single `ticks_diff` reads negative. Assumption allowed: none on pause length.

### V4 No negative or overflowing durations over any uptime
- V4.1 `ticks_diff` matches MicroPython for every pair of ticks, no panic.
- V4.2 Every duration shown to a user or used in a threshold (door-open minutes, command age,
  uptime, "closed after") is >= 0 and equals true elapsed time, for any uptime, not only while
  elapsed < 2^29 ms. main.py prints "open for -8947 minutes" after 6.2 days open.
- V4.3 Uptime never decreases and does not overflow for 100 years (3 155 760 000 s, needs
  more than 32 bits). It trails real time by less than 1 s per heartbeat at most; better,
  carry the remainder so it does not drift.
- V4.4 No arithmetic in the logic crate can overflow or panic for any input (Kani default
  checks on, all inputs unconstrained unless the spec states a bound).

### V5 A fresh command is never dropped
- V5.1 Behind `/update`: if a batch holds `/update` followed by other commands, each later
  command is still answered (executed if fresh). In main.py the OTA reboot loses the RAM
  offset and the boot flush then throws the rest away with no reply.
- V5.2 `/update` itself is confirmed to Telegram before the reboot, so it is not re-run and
  not silently eaten.
- V5.3 After a flush timeout: if the boot flush fails, commands sent after boot must not be
  discarded by a later flush. main.py announces "online", then the in-loop flush deletes
  whatever people sent in answer. Either never announce before the flush succeeds, or (better)
  drop the destructive flush and let the age check refuse old commands (silently, since 2026-11-22).
- V5.4 In general: a fresh command from the authorised chat is never discarded without a
  reply by any path (flush, reboot, batch handling, error path).

### V6 Watchdog fed or reset
- V6.1 Every loop pass feeds the WDT, and so does every blocking call before it starts.
- V6.2 Between two feeds the worst-case blocking time (sum of timeouts) is below
  300 000 ms. Otherwise the WDT resets the board, which is the accepted fallback.
- V6.3 No path leaves the bot alive, feeding the WDT, and deaf forever. main.py's
  `connect_wifi()` feeds the WDT in an infinite retry loop. Wanted: a bound on consecutive
  failed polls or reconnect time, after which the bot stops feeding and lets the WDT reset it.
- V6.4 A panic in the logic ends in a reset, not a hang.

## What is out of scope for proofs (must be listed in TRUSTED.md with a fence)

TLS and HTTP client, JSON parsing, GPIO, the WDT peripheral, NTP correctness, RTC drift
between syncs, Telegram's own `date` field, and the OTA image. Each needs a stated assumption
and a fence that catches it when false.

## Where CONTRACTS.md is weaker than this spec

Written after reading CONTRACTS.md (328 lines, no "Changes" section at the time).
"Weaker" means: my spec asks for it, the contracts do not promise it.

| # | My spec | CONTRACTS.md | Effect |
|---|---------|--------------|--------|
| G1 | V1: a reply reaches the user under ok / timeout / error | `prop_always_responds`: one reply is *handed to the shell*. The send result is not an input to the core; the reply slot is cleared when `Op::Send` is chosen | A send that times out or errors loses the reply. Same for close alert, open alert, online notice. "Responds" means one attempt |
| G2 | V1.6: never deaf for unbounded time | Reboot fence only for WiFi down 30 min. WiFi up + every poll failing (DNS, TLS, expired cert, Telegram block) has no bound | Alive, fed, deaf forever |
| G3 | V1.6 / V4: a clock that goes back is caught | The shell is trusted to give a monotonic `now_ms` (T8). No check in the core | `now < last_poll`: no poll and no reboot until the clock catches up (harness `gap_poll_due_even_if_clock_went_back`). Durations saturate to 0, so no wrong numbers |
| G4 | V5: a fresh command is never dropped | True inside one boot. `Op::Reboot` (overrun, WiFi fence, OTA) loses RAM state: pending reply, armed close request, open timer, pending /update | After a reboot the command is redelivered and dropped silently as PreBoot (not executed, no reply). A /close watch armed before the reboot is gone with no alert |
| G5 | V2: only a real "/open" opens | `parse_command` contract proven for slices up to 10 bytes; real texts go to 64 (`/open@botname` is 13+) | Beyond 10 bytes the reading of the text is unproven. My harness `t7_only_open_parses_as_open` covers 14 |
| G6 | V2: only the configured chat opens | JSON scan, `to_update_in`, text building (T7) have function contracts but no property; `difftest/` is empty | "authorized" rests on unreviewed spec lines. My harness `t7_authorized_means_configured_chat` covers the last step only |
| G7 | V2.6: never twice | Follows from offset monotonic inside one boot, and from the PreBoot check (assumption A2) across a reboot | The double-press fence across reboot is a clock assumption, not a stored offset. Stated only as part of "stale" |
| G8 | V1: every command is answered | `update_id >= MAX_UPDATE_ID` (2^62) is skipped with no reply | Not reachable with real Telegram ids. Fine, but it is a silent path |
| G9 | V5.3: first command after boot executes | A message sent in the first ~2 s after boot (NOW_SLACK_S), or whose poll took longer than its budget, is refused | Dropped silently, door not moved. Safe side. User must resend |
| G10 | V6: watchdog fed or reset | `prop_watchdog` bounds the budget of each Op. That the shell keeps each Op inside its budget, feeds only between passes, and that `Op::Reboot` resets, are shell obligations | Proven half: the core never asks for more than 240 s. Unproven half: T8 |
| G11 | Proofs are about the code that runs | 20 of 45 contract harnesses "not ready"; all 45 bodies are `unimplemented!()` at the time of writing | Every result so far is about the builder's executable spec, not firmware |

Where CONTRACTS.md is *stronger* than main.py and I agree: no boot flush (the age check refuses
instead), u64 time (no 2^30 wrap to reason about), clock from the HTTP `Date` header of the
same poll (no NTP), /update waits for an empty good poll.


## Results (2026-11-22)

All 45 builder bodies are `unimplemented!()`, so nothing could be run against firmware code.
The 48 harnesses in `logic/src/verifier_proofs.rs` were run against the builder's executable
spec instead (each call replaced by its `spec_*` function, in a private copy): 47 pass, and
1 is a known gap kept as `should_panic`.

First run had 4 failures:

| Harness | Counterexample | Verdict |
|---------|----------------|---------|
| `gap_poll_due_even_if_clock_went_back` (was `v1_...`) | `last_poll = Some(t)`, `now < t`, WiFi up, nothing pending: Op is Idle | Contract gap G3. Kept, marked `should_panic` |
| `v4_open_alert_duration_is_true_elapsed` | open alert with `now < open_since`; reported duration 0 | Wrong property (asked `now >= open_since` with no monotonic clock). Fixed: duration is `now.saturating_sub(open_since)`. Same root as G3 |
| `v5_update_needs_new_empty_poll` | start state `quiesced && batch_left > 0` | Wrong property: state is unreachable. Fixed: proved the invariant "batch open implies not quiesced" from boot through every transition |
| `v5_first_command_after_boot_executes` | poll took 60 s, so age = 63 s: Stale | Wrong property: my scenario let a poll outlast its 30 s budget. Fixed. See G9 |

To rerun on real bodies: `cargo kani -Z stubbing -Z function-contracts --harness verifier_proofs`.

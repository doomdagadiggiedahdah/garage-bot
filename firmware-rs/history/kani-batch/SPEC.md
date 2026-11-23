# GarageBot timing spec (written by the PROOF agent from main.py, before seeing the port)

Source of truth: `garage-bot/main.py` v1.6.0 and `ARCHITECTURE.md`. This is what the code is
*meant* to do; where the Python visibly falls short, it is listed under "Known gaps".

## Clock facts (MicroPython v1.26.1, ESP32 port, checked in upstream source)

- `MICROPY_PY_TIME_TICKS_PERIOD = MP_SMALL_INT_POSITIVE_MASK + 1` (py/mpconfig.h); ESP32 uses
  `MICROPY_OBJ_REPR_A` on a 32-bit word and does not override it, so **TICKS_PERIOD = 2^30 ms**
  (1 073 741 824 ms, about 12.43 days).
- `ticks_ms() = mp_hal_ticks_ms() & (PERIOD-1)`, always in `[0, 2^30)`.
- `ticks_diff(end, start) = ((end - start + PERIOD/2) & (PERIOD-1)) - PERIOD/2`, result in
  `[-2^29, 2^29 - 1]`.
- **Documented max interval**: the result equals the true elapsed time only while the true
  elapsed time is in `[0, 2^29 - 1]` ms (about 6.21 days). Beyond that it goes negative/aliases.

## Constants

INITIAL_ALERT 15 min, REPEAT_ALERT 5 min, POLL_INTERVAL 5 s, CLOSE_TIMEOUT 60 s,
MAX_COMMAND_AGE 60 s (guarded: open, close, update), HEARTBEAT 60 s, NTP_RESYNC 6 h,
WATCHDOG_TIMEOUT 300 000 ms.

## Properties

### (a) Elapsed time across wraparound
For any start tick `s` in `[0, 2^30)` and any true elapsed `d` in `[0, 2^29 - 1]`, with
`now = (s + d) mod 2^30`: `ticks_diff(now, s) == d`. In particular it is right when `now < s`
numerically. The result is always in `[-2^29, 2^29-1]` and `ticks_diff(a,b) == -ticks_diff(b,a)`
except at the `-2^29` edge. No arithmetic overflow/panic for any pair of valid ticks.

### (b) Close-timeout alert
After a bot-triggered `/close` at tick `t0` (accepted only when the door is open), on every loop
pass where the door still reads open and no close-timeout alert was sent yet: the alert
("Door did not close") is sent **iff** elapsed-since-`t0` >= 60 000 ms. Holds for any `t0`,
including when the tick counter wraps between `t0` and now. Consequences:
- never fires before 60 s; fires on the first pass at or after 60 s;
- fires at most once per close request;
- never fires if the door is seen closed first (seeing closed clears the request);
- never fires without a bot-triggered close;
- a new `/close` re-arms it.

### (c) Uptime accumulator
`uptime_seconds` only changes at a heartbeat (elapsed since last heartbeat >= 60 000 ms), where
it grows by `elapsed // 1000` (so by >= 60). It never decreases. Stored in Rust it must not
overflow for at least N = 100 years of uptime (3 155 760 000 s; needs more than i32).
It should also not fall behind real time by more than the sub-second remainder dropped per
heartbeat (< 1 s per heartbeat).

### (d) Always responds
Model: each poll (`getUpdates`) and each `sendMessage` has an outcome: success, timeout, error.
- A failed poll delivers nothing and does not advance the update offset, so the commands are
  delivered by a later poll.
- Every *fresh* recognised command from the authorised chat, once delivered, gets **exactly one**
  reply attempt (one `sendMessage` call, whatever its outcome; no retry, no double send), and is
  never processed twice (offset advances past it).
- Unknown text and other chats get zero replies.
- Commands queued while the bot was offline (the boot backlog) are discarded: never executed and
  never replied to. No command is processed until the backlog flush has succeeded.
- Age-stale guarded commands seen during the loop (open/close/update older than 60 s, clock
  synced) are never *executed* (no relay press). main.py does send one "Ignored" notice for
  these; that notice is the only message allowed.
- Exception by design: `/update` with a newer version sends several progress messages.

### (e) Watchdog (extra)
The WDT is fed at the top of each loop pass, before every HTTP call, and every second while
reconnecting WiFi. If every blocking call respects its timeout (10 s HTTP, 30 s OTA download),
the time since the last feed never reaches 300 000 ms.

### (f) Door-open alert (extra)
While the door stays open and alerts are not muted: first alert on the first pass at >= 15 min
open, then again whenever >= 5 min passed since the last alert; none before 15 min. `silence`
mutes until the door is next seen closed. The minutes shown in the alert equal the true open
time (floor), and are never negative.

### (g) Periodic work resumes after any pause (extra)
Poll (5 s) and heartbeat (60 s) are due whenever the true elapsed time since they last ran is at
least their interval, however long the loop was blocked (WiFi reconnect retries forever).

## Known gaps spotted while reading main.py (expected to show up as failures)

1. `open_start_time` is held for the whole open period. Door open longer than 2^29 ms (6.2 days):
   `elapsed_minutes` goes negative ("open for -8947 minutes"), and the "Door closed after" notice
   is skipped. Breaks (f).
2. `connect_wifi()` blocks forever while feeding the WDT. After an outage of 6.2 to 12.4 days,
   `ticks_diff(now, last_poll_time)` is negative, so polling (and heartbeat, close-timeout) stay
   silent for up to another 6.2 days. Breaks (g) and the liveness half of (d).
3. If the boot-time backlog flush fails but the "online" message is sent, commands people send in
   reply are silently thrown away by the in-loop flush. Breaks "fresh command gets a reply".
4. With the clock never synced (NTP down), the stale guard is off: an `/open` queued during a
   mid-run WiFi outage is executed late.
5. Each heartbeat drops up to 999 ms, so uptime under-reports by up to about 1.6 % at worst.

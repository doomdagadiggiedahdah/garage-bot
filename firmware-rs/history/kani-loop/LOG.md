# LOG — formal verification in the loop (garagebot-kani-loop)

One line per proof run (cache.py) and per finding (NOTE). Times are UTC.

- 2026-11-22T08:24:04Z | RUN | contract_ticks_diff | PASS | 1.0s | key 8ecb4744ffcd
- 2026-11-22T08:24:04Z | SUMMARY | ran=1 cached=0 failed=0 wall=1.0s | passing=1/1 | root 26c6c9f457027359
- 2026-11-22T08:24:24Z | RUN | contract_ticks_add | PASS | 0.8s | key 2b391506d48b
- 2026-11-22T08:24:25Z | RUN | prop_a_ticks_across_wrap | FAIL | 0.2s | key 912d3c0b1f4d
- 2026-11-22T08:24:25Z | RUN | prop_a_wrap_hazard | FAIL | 0.2s | key e7b4ada2b815
- 2026-11-22T08:24:25Z | SUMMARY | ran=3 cached=1 failed=2 wall=1.2s | passing=2/4 | root 7ddfd17dde7403ec
- 2026-11-22T08:24:32Z | NOTE | TOOLING | stub_verified needs -Z stubbing as well as -Z function-contracts; 2 runs failed at compile. Caught on the first caller proof (run 3-4); a batch run would hit it on every caller harness at once.
- 2026-11-22T08:24:33Z | RUN | prop_a_ticks_across_wrap | PASS | 0.7s | key 912d3c0b1f4d
- 2026-11-22T08:24:33Z | RUN | prop_a_wrap_hazard | PASS | 0.5s | key e7b4ada2b815
- 2026-11-22T08:24:33Z | SUMMARY | ran=2 cached=2 failed=0 wall=1.2s | passing=4/4 | root 96311b8f4cd347c2
- 2026-11-22T08:24:53Z | RUN | contract_close_timeout_due | PASS | 1.1s | key 1eca8cd64689
- 2026-11-22T08:24:53Z | RUN | prop_b_close_alert_iff_literal | FAIL | 0.7s | key cf3c98e4899e
- 2026-11-22T08:24:53Z | SUMMARY | ran=2 cached=4 failed=1 wall=1.8s | passing=5/6 | root d0716775a88b8f55
- 2026-11-22T08:25:19Z | NOTE | BUG-PROPERTY | (b) literal "fires iff open LONGER THAN timeout" fails at exactly 60000 ms: main.py:703 uses >=. Property restated as >= (prop_b_due_iff_elapsed). Found on run 8, before close_check was written.
- 2026-11-22T08:25:20Z | RUN | contract_close_check | PASS | 1.3s | key 86bd8c9e69e8
- 2026-11-22T08:25:21Z | RUN | prop_b_alert_at_most_once | PASS | 1.0s | key d3d8436248aa
- 2026-11-22T08:25:22Z | RUN | prop_b_alert_iff_open_past_timeout | PASS | 1.0s | key fc5ccfaeb036
- 2026-11-22T08:25:23Z | RUN | prop_b_due_iff_elapsed | PASS | 0.7s | key 619b04f118f3
- 2026-11-22T08:25:24Z | RUN | prop_b_missed_after_half_period | PASS | 0.9s | key 0384663780dc
- 2026-11-22T08:25:24Z | SUMMARY | ran=5 cached=5 failed=0 wall=5.0s | passing=10/10 | root 20426e2084ab11a5
- 2026-11-22T08:25:48Z | NOTE | BUG-PYTHON | (a)/(b) limit, latent: every ticks_diff(..) >= X test is false once real elapsed >= 2^29 ms (6.2 days). If the loop stalls that long after a /close (connect_wifi infinite retry feeds the WDT), the did-not-close alert is skipped for another 6.2 days. Proven: prop_a_wrap_hazard, prop_b_missed_after_half_period. Low severity.
- 2026-11-22T08:25:58Z | RUN | contract_uptime_accumulate | PASS | 9.3s | key 923b8724b951
- 2026-11-22T08:25:59Z | RUN | prop_c_uptime_tracks_real_time | FAIL | 1.5s | key eb2c1b1db298
- 2026-11-22T08:25:59Z | SUMMARY | ran=2 cached=10 failed=1 wall=10.8s | passing=11/12 | root 12e208b8d9f4ae36
- 2026-11-22T08:26:33Z | NOTE | BUG-PYTHON | (c) uptime drift: main.py:618-619 adds diff//1000 then sets last_heartbeat_time=current_time, dropping diff%1000 ms every heartbeat. uptime_seconds runs slow by up to 999 ms/min (the 0.5 s loop sleep makes ~250 ms/min typical, minutes per day); it also skews the NTP resync interval and the logged uptime. Found by failing prop_c_uptime_tracks_real_time right after the accumulator was proven; restated as exact law prop_c_uptime_drift; fix proven in uptime_accumulate_fixed.
- 2026-11-22T08:27:16Z | RUN | contract_uptime_accumulate_fixed | PASS | 43.4s | key a1c1ba505f13
- 2026-11-22T08:27:23Z | RUN | prop_c_fixed_no_drift | PASS | 6.9s | key 5c11c90db741
- 2026-11-22T08:27:38Z | RUN | prop_c_uptime_drift | PASS | 14.5s | key 81cfb1116bd9
- 2026-11-22T08:27:40Z | RUN | prop_c_uptime_monotonic | PASS | 2.3s | key d6fe2df80591
- 2026-11-22T08:30:40Z | RUN | prop_c_uptime_no_overflow | ABORTED | ~125s | killed by hand, not answered (logged manually; cache.py now has a 300 s per-harness timeout)
- 2026-11-22T08:30:05Z | NOTE | BUG-PROPERTY | prop_c_uptime_no_overflow v1 stated the invariant as u <= k*UPTIME_MAX_STEP with symbolic k: 64-bit multiplication, solver did not finish in 2 min (eE CPU was shared with the batch teams cbmc). Restated as a multiplication-free inductive step + a const assertion. In a batch run this one harness would have stalled the whole final run.
- 2026-11-22T08:30:05Z | RUN | prop_c_uptime_no_overflow | PASS | 0.8s | key e05ed0af9778
- 2026-11-22T08:30:05Z | SUMMARY | ran=1 cached=15 failed=0 wall=0.8s | passing=16/16 | root 33152f08abb2423e
- 2026-11-22T08:31:09Z | RUN | contract_stale_command_age | PASS | 1.2s | key 9ea3a44172b5
- 2026-11-22T08:31:10Z | RUN | prop_stale_filter | PASS | 0.9s | key c3b77141cff7
- 2026-11-22T08:31:10Z | SUMMARY | ran=2 cached=16 failed=0 wall=2.1s | passing=18/18 | root 0e70a2151400f206
- 2026-11-22T08:31:48Z | RUN | contract_handle_command | PASS | 1.6s | key 9e57e18dbb1f
- 2026-11-22T08:31:49Z | RUN | contract_send_telegram | PASS | 1.7s | key 9b90c2312ca1
- 2026-11-22T08:31:49Z | SUMMARY | ran=2 cached=18 failed=0 wall=3.3s | passing=20/20 | root 688fb45b9a5540a6
- 2026-11-22T08:32:50Z | RUN | contract_process_update | PASS | 17.9s | key 07214688e36d
- 2026-11-22T08:32:55Z | RUN | prop_d_literal | FAIL | 4.6s | key 0322396c54e5
- 2026-11-22T08:32:55Z | SUMMARY | ran=2 cached=20 failed=1 wall=22.5s | passing=21/22 | root 7c1346db4491a7f6
- 2026-11-22T08:33:56Z | NOTE | BUG-PROPERTY | (d) literal fails on both halves (prop_d_literal, run right after process_update was proven). (1) "stale commands get none": main.py:657 deliberately sends one Ignored-notice per stale command; the true guarantee is NOT EXECUTED + one notice. (2) "every fresh command gets exactly one": unknown text gets 0 (handle_command returns None) and /update with a new version gets 2 notices then OTA. Restated as prop_d_per_command.
- 2026-11-22T08:34:45Z | RUN | contract_poll_step | PASS | 49.2s | key 4668b75e1053
- 2026-11-22T08:34:51Z | RUN | prop_d_per_command | PASS | 5.9s | key b73b32da6deb
- 2026-11-22T08:34:51Z | SUMMARY | ran=2 cached=21 failed=0 wall=55.1s | passing=23/23 | root e4dd7285ce8686cb
- 2026-11-22T08:35:31Z | RUN | prop_c_uptime_no_overflow | PASS | 1.7s | key 3c5be59a92cf
- 2026-11-22T08:35:55Z | RUN | prop_d_fresh_commands_dropped | PASS | 23.5s | key 2f8e702293d4
- 2026-11-22T08:36:30Z | RUN | prop_d_poll_batch | PASS | 35.0s | key 8abb95396af7
- 2026-11-22T08:36:50Z | RUN | prop_wdt_poll_never_starves | PASS | 20.2s | key 477d5b87fe27
- 2026-11-22T08:36:50Z | SUMMARY | ran=4 cached=22 failed=0 wall=80.4s | passing=26/26 | root 112349b17c86f758
- 2026-11-22T08:37:15Z | NOTE | DEMO | step 0: nothing edited
- 2026-11-22T08:37:15Z | SUMMARY | ran=0 cached=26 failed=0 wall=0.0s | passing=26/26 | root 112349b17c86f758
- 2026-11-22T08:37:16Z | NOTE | DEMO | step 1: ticks_diff BODY rewritten (& MASK -> % PERIOD), contract untouched: expect only contract_ticks_diff
- 2026-11-22T08:37:18Z | RUN | contract_ticks_diff | PASS | 1.6s | key ea84d83b14ae
- 2026-11-22T08:37:18Z | SUMMARY | ran=1 cached=25 failed=0 wall=1.6s | passing=26/26 | root b7d9e94e43811e71
- 2026-11-22T08:37:19Z | NOTE | DEMO | step 2: ticks_diff CONTRACT gains a clause: expect ticks_diff + the 5 harnesses that stub it, nothing above them
- 2026-11-22T08:37:21Z | RUN | contract_close_timeout_due | PASS | 1.8s | key 2dc1aa16b43c
- 2026-11-22T08:37:22Z | RUN | contract_ticks_diff | PASS | 1.8s | key 5fbd975e5e04
- 2026-11-22T08:37:29Z | RUN | contract_uptime_accumulate | PASS | 7.1s | key c3c9f38923b1
- 2026-11-22T08:38:11Z | RUN | contract_uptime_accumulate_fixed | PASS | 41.1s | key 2509b2afffcb
- 2026-11-22T08:38:12Z | RUN | prop_a_ticks_across_wrap | PASS | 1.8s | key d47d11ecdbdc
- 2026-11-22T08:38:14Z | RUN | prop_a_wrap_hazard | PASS | 1.7s | key 60217a42eaeb
- 2026-11-22T08:38:14Z | SUMMARY | ran=6 cached=20 failed=0 wall=55.3s | passing=26/26 | root ae45dd7b12d586e7
- 2026-11-22T08:38:15Z | NOTE | DEMO | step 3: ticks_diff body MUTATED (half-period offset dropped): expect contract_ticks_diff to rerun and FAIL, root to drop it
- 2026-11-22T08:38:17Z | RUN | contract_ticks_diff | FAIL | 1.9s | key 0f9cfa84faa2
- 2026-11-22T08:38:17Z | SUMMARY | ran=1 cached=25 failed=1 wall=1.9s | passing=25/26 | root 1f3f349d14e24274
- 2026-11-22T08:38:18Z | NOTE | DEMO | step 4: all demo edits reverted: expect ran=0 and the pre-demo root
- 2026-11-22T08:38:18Z | SUMMARY | ran=0 cached=26 failed=0 wall=0.0s | passing=26/26 | root 112349b17c86f758
- 2026-11-22T08:38:34Z | SUMMARY | ran=0 cached=26 failed=0 wall=0.0s | passing=26/26 | root 112349b17c86f758

## Summary (written 2026-11-22, after the last run above)

Totals: 41 proof runs (33 while developing, 8 in the cache demo), 436 s proof wall time (311 s without the aborted run),
6 FAIL + 1 aborted. Final: 26 harnesses, all passing and sound. Root in ROOT.txt:
`112349b17c86f758a491523037e8aceb98c9bf02343f453697481a2f6a4e3f2b`

### Properties
| | result | harnesses |
|---|---|---|
| (a) tick math across wrap, period 2^30 ms | proven for real elapsed < 2^29 ms; proven WRONG (negative) beyond | prop_a_ticks_across_wrap, prop_a_wrap_hazard |
| (b) close alert iff open >= 60 s after /close | proven (>=, not >), at most once per request; missed if first pass is >= 6.2 days late | prop_b_due_iff_elapsed, prop_b_alert_iff_open_past_timeout, prop_b_alert_at_most_once, prop_b_missed_after_half_period |
| (c) uptime monotonic, no overflow | proven; but it drifts slow (finding) | prop_c_uptime_monotonic, prop_c_uptime_no_overflow, prop_c_uptime_drift, prop_c_fixed_no_drift |
| stale filter | proven: refused iff synced, guarded, dated, age > 60 s | prop_stale_filter |
| (d) one reply attempt per fresh command, stale not executed | proven in restated form, per update and per batch of <= 3, any outcome mix | prop_d_per_command, prop_d_poll_batch, prop_d_fresh_commands_dropped |
| watchdog | proven: max unfed gap in a poll = 1 HTTP call + 1 relay press, independent of batch size (assumes one HTTP call < 299 s) | prop_wdt_poll_never_starves |

### Bugs / findings by type
Python logic (main.py):
1. Uptime drift: diff % 1000 ms dropped every heartbeat (main.py:618-619). Fix proven (uptime_accumulate_fixed).
2. Fresh commands silently dropped, no reply: (i) while backlog_flushed is False the late flush discards
   commands sent after the "online" message; (ii) commands behind a successful /update in the same batch.
3. Latent: all ticks_diff comparisons invert after 2^29 ms (6.2 days) of real elapsed time; a close alert
   can be skipped if the loop stalls that long. Low severity.
Property (the stated property was wrong, the code was as designed):
4. (b) "longer than" vs >= at exactly 60000 ms.
5. (d) "stale commands get none": they get one Ignored-notice (and are not executed).
6. (d) "every fresh command gets exactly one": unknown text gets 0, /update-with-new-version gets 2 + OTA.
7. no-overflow invariant stated with a symbolic 64-bit multiply: solver did not finish; restated.
Port (Rust): none found by Kani. Every contract proof passed on its first run.
Tooling: stub_verified needs -Z stubbing (2 failed runs).

Not proven, only read: the string-level command matching (abstracted to the Cmd enum), the OTA path,
the 15-min/5-min open-door alerts, connect_wifi (it feeds the WDT inside an infinite retry, so the
watchdog cannot rescue a dead WiFi stack), and a malformed update raising KeyError (crash + reset).

### What the loop caught early that a batch run would only see at the end
- Runs 3-4: the missing -Z stubbing flag, on the first caller instead of on all 21 stubbing harnesses.
- Run 8: the (b) boundary, before close_check and its three properties were built on it.
- Run 15: the uptime drift, straight after the accumulator contract passed, so the failure could only
  be the property or the Python, not the port. Same for (d) on run 27: process_update was already
  proven against its contract when the literal property failed, which pinned both failures on the property.
- Run 20 (aborted): one non-terminating harness. Alone it cost 2 minutes; at the end of a batch it
  would have hidden every result behind it.
- Each failure had exactly one new thing in scope; no failure needed more than one fix-and-rerun.
- Cache: re-proving after a leaf body edit cost 1 of 26 harnesses (1.6 s); after a leaf contract edit
  6 of 26; a broken leaf was caught by its own proof alone; reverting restored the same root with 0 runs.

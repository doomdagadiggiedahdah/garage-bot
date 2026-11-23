# GarageBot: what is trusted, not proven

Everything in `logic/src/fns/` is to be proven equal to its specification, and
the safety properties in `logic/src/props.rs` are proven from those
specifications (`CONTRACTS.md`). This file lists what those proofs rest on.

Each entry: what it is, why it cannot be proven, where it is, the assumption
made, and the fence that limits the damage if the assumption is wrong.

Line numbers are for the commit of 2026-11-22 (69 functions).

No function body is written yet. Until the bodies exist and their proofs pass,
nothing here is a statement about firmware: the properties hold for the
specifications.

## Total

| Part | Lines today |
|---|---|
| Assumptions about the world, written in proof harnesses (T1 to T4) | 7 |
| One definition read by a person (T5) | 1 |
| Slice length limits in contract harnesses (T6) | 14 |
| **Unproven lines inside `logic/`** | **22** |
| Specification with no top-level property over it (T7) | about 930 of `spec.rs` |
| Shell: hardware and network code (T8, T13, T14) | 434 (shell/src/main.rs, non-blank non-comment) |
| Reasoning step done by a person (T10) | 0 lines, one argument |
| Purity of the logic functions, checked as text (T11) | 0 lines, one lint |
| State that a reboot loses (T12) | 0 lines, accepted behaviour |
| Secrets read from NVS at boot (T13) | part of the shell count |
| OTA: download, signature, slots, bootloader rollback (T14) | part of the shell count, plus ESP-IDF |

## T1. Telegram's `Date` header is not behind true time (A1)

- **What:** the HTTP `Date` header of a getUpdates response, used as the clock.
- **Why unprovable:** it is a number sent by a remote server.
- **Lines:** `props.rs` 137 (1 line): `kani::assume(req_s <= d)`.
- **Assumption:** the server stamped the response no earlier than the true time
  at which the request left the device, cut to whole seconds.
- **Fence:** if the server's clock is behind by X seconds, a command up to
  `STALE_AFTER_S + X` old can run; nothing else changes. `make_anchor` rejects
  dates at or above `MAX_UNIX_S` and dates below the device's own uptime. The
  shell must take the header only from a response it read over TLS from
  api.telegram.org. A clock that is ahead only makes commands look older.

## T2. The device clock does not run fast (A2)

- **What:** the monotonic millisecond clock between boot and a poll request.
- **Why unprovable:** it is the physics of the crystal oscillator.
- **Lines:** `props.rs` 131, 133, 139, 140 (4 lines, in `prop_anchor_is_sound`;
  140 is marked `// A2`; 131 and 133 only say that the ghost values are a
  well-formed time below 2^60 ms and that boot was not before 1970).
- **Assumption:** from boot to the request of the first poll that gives an
  anchor, the monotonic clock gained less than 1 s on real time. With that poll
  inside the first 30 minutes this is 550 ppm; a crystal is below 100 ppm.
- **What bounds the 30 minutes:** `prop_deaf_reboots`. A device with no
  successful poll for `DEAF_REBOOT_AFTER_MS` (30 min) since boot reboots. Not
  bounded: polls that succeed but carry no usable `Date` header keep the
  device up with no anchor. It then presses for nothing
  (`prop_no_clock_no_press`), but its first anchor can come later than 30
  minutes, and the assumption is then weaker (see "Open points").
- **Double press after a reboot rests on this.** The update offset is in RAM.
  After a reboot Telegram can deliver an already executed /open again; the
  only thing that refuses it is the pre-boot check, which uses this bound.
- **Fence:** used only for the boot bound ("was this command sent before I
  booted?"). `spec_boot_unix` adds one second for it. If it fails by X seconds,
  a command sent up to X seconds before boot could pass the pre-boot check; it
  must still pass the 60 s staleness check. 1 s is 16 ppm over 17 hours or
  100 ppm over 2.7 hours; a longer-running device should refresh this bound
  (not designed yet, see "Open points").

## T3. The device clock does not run slow (A3)

- **What:** the monotonic clock between an anchor and its use.
- **Why unprovable:** physics again.
- **Lines:** `props.rs`, `prop_stale_never_executes`: the line marked `// A3` (198) and the
  well-formedness line above it (2 lines).
- **Assumption:** over any window up to `ANCHOR_MAX_AGE_MS` (300 s) the
  monotonic clock loses at most 1 s on real time (3300 ppm; a crystal is below
  100 ppm). The clock must keep counting through any sleep mode.
- **Fence:** `judge_age` refuses anchors older than 300 s, so the error cannot
  accumulate, and adds `NOW_SLACK_S`. If the clock stalls by X seconds within a
  window, a command up to `STALE_AFTER_S + X` old can run.

## T4. `message.date` is the time the message was sent (A4)

- **What:** the `date` field Telegram puts on a message.
- **Why unprovable:** remote data.
- **Lines:** none; it is what "age of a command" means in `prop_stale_never_executes`.
  The same harness assumes the stored anchor and boot bound are sound
  (`props.rs` 188, 194). Those two lines are not trust: `prop_anchor_is_sound`
  proves them for everything `on_poll` stores, and `prop_clock_only_from_poll`
  proves nothing else writes the clock.
- **Assumption:** `date` is the true send time cut to whole seconds, on the
  same clock as the `Date` header.
- **Fence:** T1 and T4 use the same server's clock, so a constant offset of
  that clock cancels out.

## T5. `spec_whole_minutes(e)` is `e / 1000 / 60`

- **What:** the definition `pub fn spec_whole_minutes(elapsed_ms: u64) -> u64 { elapsed_ms / 1000 / 60 }`.
- **Why unprovable:** inside `prop_open_alert_schedule` the SAT solver would have
  to show that two 64-bit divisions of the same number agree (no result in
  600 s for `/ 60_000`), so that property treats the function as opaque.
- **Lines:** `spec.rs` 27 (1 line).
- **Assumption:** a reader accepts that this line computes whole minutes.
- **Fence:** `prop_open_alert_schedule` proves the alert carries this function
  of exactly `now - open_since` for every duration; `prop_open_alert_minutes`
  checks seven concrete durations, 7.57 days and 2^62 ms among them, with the
  real function.

## T6. Slices longer than the harness bound

- **What:** `contract_f` harnesses of functions with a slice argument check
  lengths up to a bound (table in `CONTRACTS.md`), while callers assume the
  contract for any length.
- **Why unprovable:** not unprovable; the bounds keep each proof within the
  time limit. They can be raised in `tools/gen_stubs.py` at the cost of time.
- **Lines:** the 14 `kani::assume(.._len <= N)` lines in `logic/src/fns/`
  (`text_push` 24, `text_push_json` 8, `render_msg` 8 and 8, `render_reply` 8,
  `render_diag` 8, `render_log` 8, `parse_command` 10, `parse_u64` 21,
  `parse_i64` 21, `parse_http_date` 30, `classify_key` 10, `name_from` 34,
  `bot_poll_done` 30).
- **`parse_command` in particular:** proven for texts up to 10 bytes. The
  longest command is 7 bytes (`/update`); a longer text is a command only with
  a `@botname` suffix or trailing text, and that path is unproven beyond 10 bytes.
- **Assumption:** a body that is right for every slice up to the bound is right
  for longer ones.
- **Fence:** none of the safety properties uses a slice function. A fault here
  can garble a message or misread an update. A misread cannot press the relay
  for a stale command: the age check runs on the parsed date and no date gives
  no press (`prop_no_clock_no_press`). A misread chat id or date is the exposure.

## T7. Specifications that no property speaks about

- **What:** the parts of `spec.rs` that define wording, parsing, the JSON scan,
  the inbox and the glue (`spec_text_*`, `spec_render_*`, `spec_parse_*`,
  `spec_http_date`, `spec_lex_step`, `spec_walk_step`, `spec_to_update_in`,
  `spec_inbox_*`, `spec_bot_*`).
- **Why unprovable:** a specification is the statement of what is wanted. The
  proofs show the code equals it, not that it is what is wanted. For the core
  (`spec_step`, `spec_on_update`, `spec_on_poll` and what they call) the
  top-level properties check the specification itself. For this part nothing does yet.
- **Lines:** `spec.rs` from `spec_text_push` to `spec_bot_http_result`, about 930 lines.
- **Assumption:** these specifications say what main.py does (minus its six bugs).
- **JSON glue:** the path from response bytes to an `UpdateIn` (`bot_feed`,
  `scan_byte`, `lex_step`, `walk_*`, `to_update_in`) is in this part. Each
  function will be proven equal to its specification; no property says the
  specification reads JSON correctly.
- **Fence:** `difftest/` is meant to run them against the MicroPython code;
  it is empty today. The relay can be pressed only through `on_update`, whose
  properties hold whatever these functions deliver. What they can get wrong is
  which chat a message is attributed to, its date, and its command word.

## T8. The shell

- **What:** the code around the core: WiFi, TLS and HTTP, the GPIO pins, the
  hardware watchdog, the clock, OTA.
- **Why unprovable:** it is hardware and vendor libraries.
- **Lines:** 434 non-blank, non-comment lines in `shell/src/main.rs` (217 before OTA and NVS secrets, see T13 and T14) (esp-idf-svc 0.53, ESP-IDF v5.3.3). Read, not proven. The watchdog is fed only at the top of the loop; WiFi connect is bounded at 20 s and never feeds it.
- **Assumption (the shell's obligations):**
  1. Per loop pass: feed the watchdog, then call `bot_step` once, then do
     exactly the one op it returns, within `op_budget_ms(op)`.
  2. Press the relay if and only if `bot_next_update` returns `true`, once, for `RELAY_PRESS_MS`.
  3. `now_ms` is a 64-bit monotonic millisecond count since boot that never wraps or goes back.
  4. `t_req_ms` is read before the request is sent; `date_hdr` is that response's `Date` header.
  5. Every body byte of a getUpdates response goes through `bot_feed` in order;
     `http_ok` is true only for status 200 with the body read to its end.
  6. `chat_id` is the configured chat; `door_open` is the reed switch reading.
  7. The watchdog is armed with `WDT_TIMEOUT_MS` and cannot be disabled.
  8. After every `Op::Send(m)`: call `bot_send_done(b, m, ok)` with the same
     `m` and the true result, before the next `bot_step`. `ok` is true only
     for status 200 from sendMessage.
  9. `now_ms` is near 0 at power-on and after every reset. The deaf fence
     counts from 0 when there was no good poll yet; a clock that starts at
     30 minutes or more reboots on the first pass, every time.
  10. TLS validates the server certificate for api.telegram.org. T1 and T4
     take the `Date` header and `message.date` from that connection.
- **What item 3 still rests on:** a clock that goes back is now caught
  (`prop_clock_back_reboots`: `now` before the previous pass orders a
  reboot). Wrapping and a clock that stands still are not caught by that.
- **Sending, what is not promised:** a send reported as failed may have
  reached Telegram, so a retry can show a message twice. After `SEND_TRIES`
  (3) failures in a row the message is dropped (`prop_send_retry`). A message
  whose slot was refilled while it was being sent is dropped in favour of the
  newer one.
- **Fence:** the shell holds no decision logic: each op is a short straight-line
  routine, and every timeout, retry, ordering and reboot decision is in the
  proven core. `prop_watchdog` and `prop_wifi_never_blocks` show the core never
  asks for an op that could outlast the watchdog.
  One exception that decides nothing: on `Op::NextUpdate` the shell peeks at the
  inbox head (`inbox_pop`, `to_update_in`, `judge_age`, all proven, on copies) only
  to write a "dropped <cmd> silently (<reason>)" log line when the core consumed an
  authorized guarded command without a reply or press. A wrong log line cannot press
  the relay or send a message (added 2026-11-22 with the silent drop).

## T9. The tools

- **What:** rustc, Kani 0.68.0, CBMC 6.11.0 and the SAT solver; Kani's
  function replacement (`kani::stub`); `cache.py` and `proofq`, which record
  which proofs passed. Kani's contract mode (`ensures`, `stub_verified`) is no
  longer used.
- **Why unprovable:** they are the prover.
- **Lines:** none of ours in the firmware.
- **Assumption:** a "VERIFICATION SUCCESSFUL" is true, and a specification may
  stand in for a function once the proof that they are equal passes (T10, T11).
- **Setting:** `no-assertion-reach-checks` is on (`logic/Cargo.toml`). Kani
  then does not report an assertion that can never be reached as such; it
  reports it as holding. The explicit `kani::cover!` lines still run.
- **Fence:** `ROOT.txt` separates `root` (proofs whose stubbed callees are all
  proven) from `conditional-root`; only `root` counts. Every property has
  reachability checks (`kani::cover!`), so contradictory assumptions fail the
  run. The `contract_f` harnesses have no `cover!`; their only assumptions are
  the slice bounds of T6 and the facts of T10.

## T10. "Proven for every such function, therefore for this one"

- **What:** the proof of a function with callees replaces every callee and the
  callee's specification by one stand-in that may return anything (`spec.rs`,
  `mod opaque`; and `minutes_opaque` in `props.rs`). 35 of the 69 function
  proofs do this, as do `prop_open_alert_schedule` and `lemma_dec_index`. The
  properties replace each function they call by its specification.
- **Why unprovable:** Kani proves each half: "f equals spec_f whatever the
  callees return, as long as equal arguments get equal answers", and "callee c
  equals spec_c". It has no way to state that the halves combine.
- **Lines:** none assumed. The argument rests on things a reader can check in
  `mod opaque` (`spec.rs` from line 1390: about 240 hand-written lines, of
  which the macro `standin!` is 75, then one generated `standin!` line per
  specification):
  1. A stand-in returns `kani::any()` restricted by at most one fact. The facts
     are: `div10_facts`; index of `dec` below 20; length from `json_escape` at
     most 6; `parse_u64` below 10^18 for at most 18 bytes. Each is asserted for
     the real specification by a lemma (`lemma_div10_facts`, `lemma_dec_index`,
     `lemma_json_escape_len`, `lemma_parse_u64_18`).
  2. A stored answer is returned only for arguments equal to the ones it was
     stored for. Equality of slices is by length and bytes (`SK`), of texts by
     `gb_text_eq`, which `lemma_text_eq` shows to be `==`.
  3. `opaque::begin()` empties every table, so no answer survives from an
     earlier use.
  4. The real callee is a function of its arguments only (T11), so "equal
     arguments, equal answers" is true of it.
- **Assumption:** a property proven for every function with those facts holds
  for the real function, which has them.
- **Fence:** the stand-ins are compiled only under Kani, never into firmware.
  The verifier's harnesses (`verifier_proofs.rs`) call the functions with no
  stand-in; once bodies exist they check the same ground a second way.

## T11. The logic functions are pure, checked as text

- **What:** replacing `f` by `spec_f` in a caller's proof is right only if `f`
  reads and writes nothing but its arguments and result.
- **Why unprovable:** the plain proof of `f` compares one call with one call of
  the specification. A body that kept a counter in a `static` could pass that
  and still answer differently on a second call.
- **Lines:** none. `cache.py` refuses to run when a file in `logic/src/fns/`
  contains `static`, `unsafe`, `Atomic`, `Cell`, `RefCell`, `UnsafeCell` or
  `extern`.
- **Assumption:** safe Rust without those words, taking arguments by value or
  shared reference, cannot keep state between calls. The check is on text, not
  on the compiled program.
- **Fence:** `logic` is `no_std` with no dependencies, so there is no library
  that could hide state. A reviewer of a body should still look for it.

## T12. A reboot loses what is in RAM

- **What:** `Op::Reboot` is the fence for five things: a late pass, a clock
  that went back, WiFi down for 30 minutes, no successful poll for 30 minutes,
  and `/update`. A reboot starts from `BOOT_STATE`.
- **Lost:** the armed /close watch (no warning if the door then fails to
  close), the open-door timer (it restarts from the first pass after boot, so
  an alert comes late), queued replies and alerts, the retry count, the update
  offset and the anchor.
- **Why unprovable:** not unprovable; it is how the device is built. Keeping
  the state would need flash writes, which main.py does not do either.
- **Assumption:** accepted. After a reboot the bot says "online", which tells
  the chat that earlier watches are gone.
- **Fence:** a lost offset can redeliver a command; the pre-boot check refuses
  it (T2). Nothing in RAM is needed to keep the relay from pressing.

## T13. Secrets come from NVS, not from the image

- **What:** WiFi SSID and password, Telegram token, chat id, NOTIFY list and the
  UDP log address are read at boot from the `nvs` partition (0x9000, namespace
  `gb`: `wifi_ssid`, `wifi_pass`, `tg_token`, `chat_id`, `notify`, `log_addr`)
  by `load_secrets` in `shell/src/main.rs`. The image is built with no secrets
  (`shell/build.rs` bakes in only the short proof root). The laptop's
  `build_and_flash.sh` makes the NVS image locally from `secrets.py` with
  `esp_idf_nvs_partition_gen`; secrets never go to eE or into git.
- **Why unprovable:** flash contents and the vendor NVS library.
- **Assumption:** the NVS library returns what was written; `chat_id` parses
  as i64 (T8 item 6 now reads "the chat id in NVS"). The partition is opened
  with `take_with(false)`, so the firmware never erases it.
- **Fence:** missing or unparseable secrets: a serial message, then a restart
  after 30 s; no network use. Nothing received over the network writes NVS
  (no command changes a secret; the only writers are esptool over USB and the
  WiFi driver's own namespace). `build_and_flash.sh` checks the app `.bin` for
  the old `GB_*` names and for a bot-token-shaped string before it prints any
  flash command.

## T14. Over-the-air update: what is proven and what is ESP-IDF

- **Proven (logic/):** `version_newer` (strictly newer, MAJOR.MINOR.PATCH),
  `parse_version` (`[v]X.Y.Z[+build]`, at most `VERSION_CAP` bytes),
  `ota_plan` (download only if strictly newer, `0 < size <= slot`, and not the
  version the bootloader last rolled back), `ota_accept` (boot it only if
  `esp_ota_end` accepted it, its size and SHA-256 equal the manifest's, and the
  version inside the image equals the manifest's), `ota_mark_valid` (a pending
  image is marked valid only after a good poll in this boot), and the update
  timer in `bot_step` (check at boot and every `OTA_CHECK_INTERVAL_MS`, 6 h).
  Properties: `prop_ota_only_newer`, `prop_ota_checks_image`,
  `prop_unverified_needs_good_poll`, `prop_step_keeps_unverified`,
  `prop_ota_check_cannot_starve`.
- **Trusted (shell and ESP-IDF v5.3.3):**
  1. Manifest parsing in `parse_manifest` (key=value lines; the version goes
     through the proven `parse_version`). A wrong parse can only produce a wrong
     `Manifest`; `ota_accept` still needs the signed image to match it.
  2. HTTPS to github.com and its redirect to the release asset host, checked
     against the ESP-IDF CA bundle; esp-idf-svc follows GET redirects.
  3. The running SHA-256 (`mbedtls_sha256_*`) covers exactly the bytes written
     with `esp_ota_write`, and no byte past the manifest size is written.
  4. Signature: built with `CONFIG_SECURE_SIGNED_APPS_NO_SECURE_BOOT`,
     ECDSA P-256 (V1 scheme) and `CONFIG_SECURE_SIGNED_ON_UPDATE_NO_SECURE_BOOT`.
     `esp_ota_end` rejects an image not signed by the key on eE
     (`~/.garagebot-signing/signing_key.pem`, outside git). The public key is
     compiled into the app. There is no secure boot and no flash encryption,
     and no eFuse is written, so anyone with USB access can still flash anything.
  5. `esp_ota_get_partition_description` returns the version string of the
     image just written.
  6. Bootloader rollback (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`): a new
     image boots `PENDING_VERIFY`. If it resets before
     `esp_ota_mark_app_valid_cancel_rollback`, the bootloader marks it invalid
     and boots the previous slot. Afterwards `esp_ota_get_last_invalid_partition`
     names it, so `ota_plan` will not fetch that version again.
     `APP_ANTI_ROLLBACK` is off (it burns eFuses).
  7. The download runs inside one loop pass with a 200 s deadline
     (`MAX_OP_BUDGET_MS` is 240 s, the watchdog 300 s). It does not feed the watchdog.
- **Fence:** if a pending image never polls, the deaf fence reboots it (T12),
  and the bootloader's rollback then takes over. No relay logic is involved.

## Open points

- A2 (T2): the 30 minute bound holds only when the first good poll carries a
  usable `Date`. A device with good polls and no anchor is not rebooted.
- No property yet covers the glue from bytes to `UpdateIn` (T7).
- All 69 function proofs pass with a reference body within 900 s and 4 GB
  (`CONTRACTS.md`, "Changes"). `text_push` has the least margin.
- The rewritten specifications of the split functions were not compared
  mechanically with the old ones; the evidence is that the properties still pass.
- Taken from the verifier's list (below): send result reported to the core
  (cut 1), reboot when the clock goes back (cut 4), reboot when deaf (cut 5).
- Not taken yet, still to do:
  - Do: make the relay press an `Op::Press`, or a value only `on_update` can
    build (cut 2).
  - Do: keep the poll's start time in `State` and drop the `t_req_ms`
    parameter (cut 3).
  - Do: cross-check successive `Date` headers against the monotonic clock (T1).
  - Do: confirm the offset to Telegram before pressing, or keep it in flash,
    so the pre-boot check stops carrying safety (T2).
  - Do: list every `kani::assume` in `props.rs` as invariant or input
    restriction (T10).
  - Do: a mutation check of `judge_age` and `door_tick` once bodies exist (T9).
  - Do: the shell obligations the verifier adds under T8 (relay pin low at
    boot, panic resets, no feeding inside a blocking call). OTA image
    authenticity is now T14 item 4 (signed images, no secure boot).

## Verifier challenges

Written by the verifier (independent of the builder), 2026-11-22. For each entry: can it be
proven or moved into the proven crate, is the assumption the weakest that works, and what
catches it when it is false. "Do" lines are ordered by how much trust they remove.

Evidence: my 48 harnesses (`logic/src/verifier_proofs.rs`) run against the executable spec,
since no body is written yet. Details in `VERIFIER_SPEC.md`.

### Biggest cuts first

1. **Send result is invisible to the core (not listed above; add as T11).** The reply slot is
   cleared when `Op::Send` is chosen. A failed send loses the reply or the close alert, and
   no proof can see it. Do: add `bot_send_done(ok: bool)` and keep the message until `ok`,
   with a proven retry bound. This moves "always responds" from the shell into the crate.
2. **Pressing the relay is a shell obligation (T8).** Do: make it an `Op::Press` (or a value
   of a type only `on_update` can build, which the shell's `press()` must take). Then "the
   shell presses only when told" is checked by the compiler, not by reading.
3. **`t_req_ms` comes from the shell (T8).** The freshness proof needs it to be the time the
   poll request began. The core already picks `Op::Poll` at a known `now`. Do: store that in
   `State` and drop the parameter. One obligation gone, no cost.
4. **No fence for a clock that goes back (T8, monotonic `now_ms`).** Counterexample from
   `gap_poll_due_even_if_clock_went_back`: `last_poll = Some(t)`, `now < t`, WiFi up, nothing
   pending: Op is Idle, for as long as `now < t + 5 s`. Do: `now < last_step` gives
   `Op::Reboot`. Then monotonicity is checked at run time and can leave this file.
5. **No bound on "WiFi up, every poll fails" (not listed; add).** Only WiFi-down reboots
   after 30 min. Expired CA, DNS, a block: alive and deaf forever, watchdog happy. Do:
   `last_good_poll` in `State`, reboot after N minutes without one. Provable like
   `prop_wifi_never_blocks`.

### T1. Date header not behind true time (A1)

- Provable: no, it is about Telegram's servers.
- Weakest: no. The proofs need only "not behind by more than x"; with x folded into the
  60 s limit (accept age < 60 - x) the assumption becomes "not behind by more than x s",
  which is weaker and honest about proxies and cached responses.
- Fence when false: none today. A stale `Date` makes old commands look fresh, which is the
  one thing the bot must not do. Do: (a) take the largest `Date` seen, never a smaller one
  (a replayed or cached response then cannot move the clock back); (b) cross-check two
  anchors: Date delta and monotonic delta must agree within slack, else drop the anchor
  (NoClock, refuses /open); (c) `message.date > Date + slack` means the header is behind:
  drop the anchor. All three are pure and provable in the crate.
- Also trusted here and not written down: TLS certificate validation. Without it, anyone on
  the path sets `Date`, and writes the updates too. Add to T8 as its own line.

### T2. Device clock not fast, more than 1 s from boot to request (A2)

- Weakest: no. A fixed 1 s over an unbounded span is false on any crystal: 50 ppm is 1 s
  after 5.5 hours. If the first good poll comes late (WiFi down at boot, reboot fence at
  30 min: 30 min at 50 ppm is 0.09 s, fine) it holds only because of that fence. Say so:
  the assumption should read "fast by less than 1 s over at most 30 min (550 ppm)", and the
  30 min bound should be a proven property (time from boot to first anchor, or reboot).
- What it protects: the PreBoot check, which is the only thing that stops a redelivered
  /open from pressing again after a reboot (the offset lives in RAM). That is a double
  press on a garage door, so this is the most safety-relevant assumption in the file.
- Remove it instead: confirm the offset to Telegram *before* acting (poll with
  `offset = id + 1`, press only after that poll returns ok). Then a reboot cannot redeliver
  an executed command and PreBoot stops carrying safety. Cost: one extra round trip per
  /open. Alternative: offset in flash.

### T3. Device clock not slow, more than 1 s per 300 s (A3)

- Weakest: close. 1 s in 300 s is 3300 ppm, generous. Fine as an assumption.
- Fence: the anchor expires after 300 s (proven). Good. The T1 cross-check (b) would also
  catch a clock that stops or jumps, which this assumption excludes only on paper.

### T4. `message.date` is the send time (A4)

- Not provable, and the weakest form. It is Telegram's server stamp, not the sender's.
- Check the shell or JSON scan drops `edited_message` and `channel_post`: an edit carries
  the original `date`, so it is refused as stale (safe), but that should be a stated
  property, not luck.

### T5. `spec_whole_minutes(e)` is `e / 1000 / 60`

- Provable, and proven on a bounded range: `v4_whole_minutes_floor` checks
  `m * 60_000 <= e < (m + 1) * 60_000` for every `e < 2^40` ms (34 years), against the spec.
  This entry can shrink to "for uptimes over 34 years, by reading". Take the harness.

### T6. Slice length bounds

- `parse_command` at 10 bytes is too low: "/open@name" in a group is longer, and this
  function decides whether the door moves. `t7_only_open_parses_as_open` covers 14 bytes
  (enough for "/open@" plus a suffix and for trailing bytes). Do: restructure so the proof
  is length independent: compare a fixed prefix of at most 6 bytes, then one loop that only
  checks "rest is whitespace or follows @". Kani loop contracts (`-Z loop-contracts`) or an
  induction step harness (state after k bytes + one byte) removes the bound.
- Fixed-format parsers (`parse_http_date`: exactly 29 bytes; `parse_u64`: at most 20
  digits): reject other lengths first. The bound is then the whole domain, not an
  assumption. Same for `name_from` (cap 32) and anything capped by `TEXT_CAP`.
- For text builders, a wrong long output is cosmetic. Say that, and keep them out of the
  safety count.

### T7. About 930 spec lines with no property

- This is the largest unproven piece and `difftest/` is empty. Most of it does not need
  trust if a few properties are added, because only three facts matter for the door:
  1. Open only from "/open": `t7_only_open_parses_as_open` (mine).
  2. Authorized only for the configured chat, id and date passed through:
     `t7_authorized_means_configured_chat` (mine).
  3. The JSON scan cannot be steered by message content: a `"chat":{"id":...}` or
     `"date":...` *inside a text string* must not be read as the real field. Needs a
     property: the scan returns the same ids for any bytes placed inside a string value.
     If that is hard to prove, the cheap fence is: reject any update whose text contains
     `"` or `\` or `{`. Provable in a line, and the bot's commands never contain them.
- Everything else here (status wording, diag text) is cosmetic: mark it so, and the
  trusted spec shrinks to the three items above.

### T8. Shell obligations (shell not written)

- Items 1 to 3 of "Biggest cuts" remove or fence three obligations. Left after that, and
  not provable: GPIO reads the real reed switch, the HTTP client honours its timeout,
  `Op::Reboot` resets.
- Missing obligations, add them: TLS validates the certificate; relay pin is driven low
  before anything else at boot (a floating pin at reset opens the door with no command);
  OTA image is authentic; a panic resets rather than hangs; the watchdog is armed before
  the first network call and is never fed inside a blocking call; the shell does not feed
  after `Op::Reboot`; press duration is bounded even if the task is preempted.
- Fence: the hardware watchdog covers hangs only. It does not cover a shell that presses
  when not told, which is why cut 2 matters.
- Suggest the shell be one loop of under about 100 lines with no branching on message
  content, so that "by reading" is possible.

### T9. Tools

- Not reducible, but two cheap checks: (a) a final full run with no cache, on a clean
  checkout, logged; (b) my harnesses call the real functions with no `stub_verified`, so
  they do not depend on contract replacement being sound. When bodies exist, both sets
  passing covers T10 as well.
- Add a mutation check: flip one comparison in `judge_age` and in `door_tick`, confirm a
  harness fails. Proves the harnesses are not vacuous. Mine use `kani::cover!` for this.

### T10. Stand-in argument

- Fine in principle. It is only as good as each contract being proven with no extra
  `assume`. List here every `kani::assume` in `props.rs` and each `requires`, and for each
  say whether it is an invariant (then prove it is kept: boot state has it, every
  transition keeps it) or a restriction on inputs (then it belongs under T1 to T8).
  Example I had to do myself: "batch open implies not quiesced" is needed for /update
  safety, and is an invariant (`v5_update_needs_new_empty_poll`).

### Open points

- 45 bodies are `unimplemented!()`. Until they exist, this file should say at the top that
  nothing about firmware is proven yet.
- Reboot is used as the fence for three things (overrun, WiFi, and proposed above: deaf,
  clock back). Reboot loses the armed /close watch, the open timer and pending replies.
  Either accept and list it, or send "rebooting, close watch lost" as the Online notice.

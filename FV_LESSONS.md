# Formal verification: lessons learned (GarageBot Rust)

## 2026-11-22, Haiku fleet 1 (25 fns × 5)
1. **Copy-only tasks aren't a benchmark.** When the contract is `ret == spec_x(...)` and spec_x is already
   code, the builder just copies it (e.g. interval_due == spec_due, line for line). 119/120 passed first try.
   → Before fanning out, label each function "copy" (exact spec exists) or "real" (only properties).
   Benchmark models only on "real". Fill in copy ones with one cheap agent, or by script.
2. **Proof setup fails look like model fails.** on_update went 0/5: every run ran out of memory before
   Kani gave a verdict. That's the harness/contract (full-State equality), not the model.
   → Before fanning out, prove each harness once with a reference body (the spec copy) and record its
   time and memory. A harness that can't pass with the reference body goes back to the architect, not to builders.
3. **Canary first.** One builder per function first, then look at the results, then fan out.
   Lessons 1 and 2 would have shown up ~30 min earlier.
4. **The contracts are the bottleneck, not the bodies.** Bodies took about an hour; the architect's split of the
   heavy functions is still running. Spend parallelism on contract work (helper architects) early.
5. **Small remote workers don't help heavy proofs.** Jetsons (8 GB shared) ran out of memory on most live jobs and
   requeued them to eE, which is slower than not having them. Use them only for small/leaf harnesses.

6. **(owner, 2026-11-22 17:00) The goal is a verified GarageBot that stops failing, plus a map of what FV can do.**
   Easy cases are a finding, not waste: when the spec is executable code and the promise is `result == spec_x`,
   the body is trivial (Haiku 95/95 on leaves). So write the hard thinking into spec form, and let
   cheap agents fill the bodies. Record each "if it's in this form, it's trivial" pattern here as found.

## 7. The real-world bug was outside the proof (2026-11-22, first flash)
Boot log of the old MicroPython bot: WiFi and NTP fine, but every HTTPS call failed with mbedTLS errors
(`PK_INVALID_PUBKEY`, `X509_INVALID_EXTENSIONS+ASN1_ALLOC_FAILED`): the TLS handshake ran out of RAM (~113 KB free).
That alone explains "the bot doesn't respond", and none of the 6 logic bugs is it. It lives in TRUSTED T8 (the shell: chip and TLS).
Rust firmware (esp-idf, cert bundle): 7/7 HTTPS OK, ~185 KB free. Logs: flash-backup/old-boot-log.txt, rust-boot-log.txt.
Lesson: capture a boot log before any proof work. FV makes the logic right; the bug a user sees can sit in the trusted part.

# GarageBot firmware: agent experiment log

## Fleet 1 (Haiku)
- Model: Haiku. Date: 2026-11-22. Wall time: ~56 min. Subagent tokens: ~7.3M total (not broken down per builder).
- 125 builders = 25 functions x k=1..5. Each wrote one function body in worktree `~/wt/haiku/<fn>-<k>` (branch `haiku/<fn>-<k>`, base e31ec26) and proved it with Kani via proofq. 6 local Kani slots (MemoryMax=4G each).
- Why Haiku: chosen for its speed plus many retries, since the compiler and Kani give exact feedback.
- Result: 120/125 passed. All 5 failures are `on_update` (0/5): every attempt ran out of memory at 4G in the Kani stage, before any verdict (no counterexample, body compiles). That is a harness/contract problem, not a Haiku miss.
- Row notes: `k` is the branch number (the builders' self-reported k was unreliable, so rows are matched to branches by commit). `runs` = proofq runs until pass as reported by the builder. `kani s` = Kani seconds of the builder's last run when it reported them (n/a otherwise). `wall s` = builder start to end. `body lines` = non-blank lines between `// @body` and `// @end`, counted from the branch. Leaf = FUNCTIONS.json `callees` is empty.

### Audit method
For every branch: `git diff e31ec26..haiku/<fn>-<k>` names only `logic/src/fns/<fn>.rs`; the file with the body region masked (and unwind numbers normalised) is identical to base; unwind not lowered; body does not mention `spec_*`. All 125 branches were checked (failed ones too).

### Per-builder rows
| fn | k | leaf | runs | kani s | wall s | passed | body lines | audit flag |
|---|---|---|---|---|---|---|---|---|
| uptime_s | 1 | y | 1 | n/a | 55 | y | 2 | - |
| uptime_s | 2 | y | 1 | n/a | 15 | y | 2 | - |
| uptime_s | 3 | y | 1 | n/a | 12 | y | 2 | - |
| uptime_s | 4 | y | 1 | n/a | 12 | y | 2 | - |
| uptime_s | 5 | y | 1 | n/a | 12 | y | 2 | - |
| interval_due | 1 | y | 1 | n/a | 60 | y | 5 | - |
| interval_due | 2 | y | 1 | n/a | 20 | y | 5 | - |
| interval_due | 3 | y | 1 | n/a | 17 | y | 5 | - |
| interval_due | 4 | y | 1 | n/a | 23 | y | 5 | - |
| interval_due | 5 | y | 1 | n/a | 15 | y | 5 | - |
| whole_minutes | 1 | y | 1 | n/a | 298 | y | 2 | - |
| whole_minutes | 2 | y | 1 | n/a | 13 | y | 2 | - |
| whole_minutes | 3 | y | 1 | n/a | 13 | y | 2 | - |
| whole_minutes | 4 | y | 1 | n/a | 444 | y | 3 | - |
| whole_minutes | 5 | y | 1 | n/a | 14 | y | 2 | - |
| tenths_of_minute | 1 | y | 2 | n/a | 924 | y | 2 | - |
| tenths_of_minute | 2 | y | 1 | n/a | 421 | y | 2 | - |
| tenths_of_minute | 3 | y | 1 | n/a | 562 | y | 4 | - |
| tenths_of_minute | 4 | y | 1 | n/a | 492 | y | 3 | - |
| tenths_of_minute | 5 | y | 1 | n/a | 257 | y | 3 | - |
| make_anchor | 1 | y | 1 | n/a | 105 | y | 6 | - |
| make_anchor | 2 | y | 1 | n/a | 20 | y | 6 | - |
| make_anchor | 3 | y | 1 | n/a | 18 | y | 6 | - |
| make_anchor | 4 | y | 1 | n/a | 348 | y | 7 | - |
| make_anchor | 5 | y | 1 | n/a | 20 | y | 6 | - |
| judge_age | 1 | y | 1 | n/a | 120 | y | 22 | - |
| judge_age | 2 | y | 1 | n/a | 293 | y | 22 | - |
| judge_age | 3 | y | 1 | n/a | 426 | y | 22 | - |
| judge_age | 4 | y | 1 | n/a | 418 | y | 23 | - |
| judge_age | 5 | y | 1 | n/a | 27 | y | 22 | - |
| decide | 1 | y | 1 | n/a | 237 | y | 31 | - |
| decide | 2 | y | 1 | n/a | 197 | y | 31 | - |
| decide | 3 | y | 1 | n/a | 26 | y | 31 | - |
| decide | 4 | y | 1 | n/a | 421 | y | 30 | outside-body-change |
| decide | 5 | y | 1 | n/a | 36 | y | 31 | - |
| on_update | 1 | n | 5 | n/a | 1237 | n | 22 | - |
| on_update | 2 | n | 3 | n/a | 245 | n | 25 | - |
| on_update | 3 | n | 2 | n/a | 523 | n | 23 | - |
| on_update | 4 | n | 2 | n/a | 460 | n | 25 | - |
| on_update | 5 | n | 2 | n/a | 240 | n | 21 | - |
| on_poll | 1 | n | 1 | n/a | 551 | y | 20 | - |
| on_poll | 2 | n | 1 | n/a | 47 | y | 20 | - |
| on_poll | 3 | n | 1 | n/a | 164 | y | 20 | - |
| on_poll | 4 | n | 1 | n/a | 30 | y | 20 | - |
| on_poll | 5 | n | 1 | n/a | 30 | y | 20 | - |
| op_budget_ms | 1 | y | 1 | n/a | 27 | y | 10 | - |
| op_budget_ms | 2 | y | 1 | n/a | 17 | y | 10 | - |
| op_budget_ms | 3 | y | 1 | n/a | 18 | y | 10 | - |
| op_budget_ms | 4 | y | 1 | n/a | 20 | y | 10 | - |
| op_budget_ms | 5 | y | 1 | n/a | 54 | y | 10 | - |
| pick_op | 1 | n | 1 | n/a | 111 | y | 35 | - |
| pick_op | 2 | n | 1 | n/a | 26 | y | 35 | - |
| pick_op | 3 | n | 1 | n/a | 158 | y | 35 | - |
| pick_op | 4 | n | 1 | n/a | 27 | y | 35 | - |
| pick_op | 5 | n | 1 | n/a | 192 | y | 39 | - |
| div10 | 1 | y | 1 | n/a | 66 | y | 2 | - |
| div10 | 2 | y | 1 | n/a | 16 | y | 2 | - |
| div10 | 3 | y | 1 | n/a | 17 | y | 2 | - |
| div10 | 4 | y | 1 | n/a | 15 | y | 2 | - |
| div10 | 5 | y | 1 | n/a | 139 | y | 4 | - |
| dec | 1 | n | 1 | n/a | 154 | y | 14 | - |
| dec | 2 | n | 1 | n/a | 23 | y | 14 | - |
| dec | 3 | n | 1 | n/a | 24 | y | 14 | - |
| dec | 4 | n | 1 | n/a | 27 | y | 14 | - |
| dec | 5 | n | 1 | n/a | 24 | y | 14 | - |
| parse_command | 1 | y | 1 | n/a | 114 | y | 44 | - |
| parse_command | 2 | y | 1 | n/a | 108 | y | 44 | - |
| parse_command | 3 | y | 1 | n/a | 320 | y | 43 | - |
| parse_command | 4 | y | 1 | n/a | 241 | y | 44 | - |
| parse_command | 5 | y | 1 | n/a | 219 | y | 45 | - |
| parse_u64 | 1 | y | 1 | n/a | 61 | y | 15 | - |
| parse_u64 | 2 | y | 1 | n/a | 19 | y | 15 | - |
| parse_u64 | 3 | y | 1 | n/a | 19 | y | 15 | - |
| parse_u64 | 4 | y | 1 | n/a | 18 | y | 15 | - |
| parse_u64 | 5 | y | 1 | n/a | 22 | y | 15 | - |
| parse_i64 | 1 | n | 1 | n/a | 96 | y | 8 | - |
| parse_i64 | 2 | n | 1 | n/a | 79 | y | 10 | - |
| parse_i64 | 3 | n | 1 | n/a | 271 | y | 19 | - |
| parse_i64 | 4 | n | 1 | n/a | 21 | y | 10 | - |
| parse_i64 | 5 | n | 1 | n/a | 240 | y | 19 | - |
| parse_http_date | 1 | y | 1 | n/a | 133 | y | 64 | - |
| parse_http_date | 2 | y | 1 | n/a | 135 | y | 65 | - |
| parse_http_date | 3 | y | 1 | n/a | 205 | y | 62 | - |
| parse_http_date | 4 | y | 1 | n/a | 227 | y | 62 | - |
| parse_http_date | 5 | y | 1 | n/a | 226 | y | 59 | - |
| lex_step | 1 | y | 1 | n/a | 40 | y | 54 | - |
| lex_step | 2 | y | 1 | n/a | 98 | y | 54 | - |
| lex_step | 3 | y | 1 | n/a | 224 | y | 54 | - |
| lex_step | 4 | y | 1 | n/a | 133 | y | 52 | - |
| lex_step | 5 | y | 1 | n/a | 169 | y | 54 | - |
| classify_key | 1 | y | 1 | n/a | 38 | y | 30 | - |
| classify_key | 2 | y | 1 | n/a | 75 | y | 14 | - |
| classify_key | 3 | y | 1 | n/a | 162 | y | 44 | - |
| classify_key | 4 | y | 1 | n/a | 18 | y | 30 | - |
| classify_key | 5 | y | 1 | n/a | 24 | y | 30 | - |
| name_from | 1 | y | 1 | n/a | 43 | y | 20 | - |
| name_from | 2 | y | 1 | n/a | 48 | y | 17 | - |
| name_from | 3 | y | 1 | n/a | 28 | y | 17 | - |
| name_from | 4 | y | 1 | n/a | 162 | y | 16 | - |
| name_from | 5 | y | 1 | n/a | 103 | y | 16 | - |
| scan_finish | 1 | y | 1 | n/a | 51 | y | 6 | - |
| scan_finish | 2 | y | 1 | n/a | 37 | y | 2 | - |
| scan_finish | 3 | y | 1 | n/a | 84 | y | 3 | - |
| scan_finish | 4 | y | 1 | n/a | 22 | y | 2 | - |
| scan_finish | 5 | y | 1 | n/a | 160 | y | 2 | - |
| to_update_in | 1 | n | 1 | n/a | 1517 | y | 15 | - |
| to_update_in | 2 | n | 1 | n/a | 1351 | y | 15 | - |
| to_update_in | 3 | n | 1 | n/a | 2051 | y | 14 | - |
| to_update_in | 4 | n | 1 | n/a | 199 | y | 15 | - |
| to_update_in | 5 | n | 1 | n/a | 23 | y | 15 | - |
| inbox_push | 1 | y | 1 | n/a | 253 | y | 9 | - |
| inbox_push | 2 | y | 1 | n/a | 242 | y | 8 | - |
| inbox_push | 3 | y | 1 | n/a | 213 | y | 9 | - |
| inbox_push | 4 | y | 1 | n/a | 294 | y | 9 | - |
| inbox_push | 5 | y | 1 | n/a | 288 | y | 9 | - |
| inbox_pop | 1 | y | 1 | n/a | 255 | y | 9 | - |
| inbox_pop | 2 | y | 1 | n/a | 288 | y | 9 | - |
| inbox_pop | 3 | y | 1 | n/a | 235 | y | 9 | - |
| inbox_pop | 4 | y | 1 | n/a | 23 | y | 9 | - |
| inbox_pop | 5 | y | 1 | n/a | 20 | y | 9 | - |
| bot_http_result | 1 | y | 1 | n/a | 149 | y | 11 | - |
| bot_http_result | 2 | y | 1 | n/a | 221 | y | 11 | - |
| bot_http_result | 3 | y | 1 | n/a | 18 | y | 11 | - |
| bot_http_result | 4 | y | 1 | n/a | 16 | y | 11 | - |
| bot_http_result | 5 | y | 1 | n/a | 13 | y | 11 | - |

## Per-function summary
| fn | leaf | passed | mean runs | mean wall s | body lines (min-max) | smallest unflagged passer re-proved | re-prove | divergence |
|---|---|---|---|---|---|---|---|---|
| uptime_s | y | 5/5 | 1.0 | 21 | 2-2 | haiku/uptime_s-1 | PASS 63.1s (j0511) | none seen (cosmetic variants) |
| interval_due | y | 5/5 | 1.0 | 27 | 5-5 | haiku/interval_due-1 | PASS 1.9s (j0470) | none seen (cosmetic variants) |
| whole_minutes | y | 5/5 | 1.0 | 156 | 2-3 | haiku/whole_minutes-1 | PASS 206.7s (j0512) | none seen (cosmetic variants) |
| tenths_of_minute | y | 5/5 | 1.2 | 531 | 2-4 | haiku/tenths_of_minute-1 | PASS 238.9s (j0509) | none seen (cosmetic variants) |
| make_anchor | y | 5/5 | 1.0 | 102 | 6-7 | haiku/make_anchor-1 | PASS 46.8s (j0486) | none seen (cosmetic variants) |
| judge_age | y | 5/5 | 1.0 | 257 | 22-23 | haiku/judge_age-1 | PASS 56.5s (j0471) | none seen (cosmetic variants) |
| decide | y | 5/5 | 1.0 | 183 | 30-31 | haiku/decide-1 | PASS 6.2s (j0467) | none seen; k=4 left a stray `// @end` inside the body (flagged) |
| on_update | n | 0/5 | 2.8 | 541 | 21-25 | none (no passer) | - | n/a (no passer) |
| on_poll | n | 5/5 | 1.0 | 164 | 20-20 | haiku/on_poll-1 | PASS 362.9s (j0488) | none seen (cosmetic variants) |
| op_budget_ms | y | 5/5 | 1.0 | 27 | 10-10 | haiku/op_budget_ms-1 | PASS 2.3s (j0489) | none seen (cosmetic variants) |
| pick_op | n | 5/5 | 1.0 | 103 | 35-39 | haiku/pick_op-1 | PASS 343.4s (j0500) | none seen (k=5 inlines interval_due; same behaviour) |
| div10 | y | 5/5 | 1.0 | 51 | 2-4 | haiku/div10-1 | PASS 14.3s (j0463) | none seen (cosmetic variants) |
| dec | n | 5/5 | 1.0 | 50 | 14-14 | haiku/dec-1 | PASS 52.2s (j0464) | none seen (cosmetic variants) |
| parse_command | y | 5/5 | 1.0 | 200 | 43-45 | haiku/parse_command-3 | PASS 36.0s (j0490) | bounded harness (t<=10); k=2,3 read, same logic |
| parse_u64 | y | 5/5 | 1.0 | 28 | 15-15 | haiku/parse_u64-1 | PASS 317.9s (j0498) | none seen (cosmetic variants) |
| parse_i64 | n | 5/5 | 1.0 | 141 | 8-19 | haiku/parse_i64-1 | PASS 24.3s (j0492) | none seen (cosmetic variants) |
| parse_http_date | y | 5/5 | 1.0 | 185 | 59-65 | haiku/parse_http_date-5 | PASS 34.9s (j0491) | read k=1 vs 2 only, cosmetic; rest pass the same bounded (s<=30) contract |
| lex_step | y | 5/5 | 1.0 | 133 | 52-54 | haiku/lex_step-4 | PASS 3.7s (j0485) | not read in detail (5 variants); all pass the same contract |
| classify_key | y | 5/5 | 1.0 | 63 | 14-44 | haiku/classify_key-2 | PASS 5.8s (j0465) | none seen (cosmetic variants) |
| name_from | y | 5/5 | 1.0 | 77 | 16-20 | haiku/name_from-4 | PASS 6.0s (j0487) | none seen (cosmetic variants) |
| scan_finish | y | 5/5 | 1.0 | 71 | 2-6 | haiku/scan_finish-2 | PASS 66.7s (j0508) | none seen (cosmetic variants) |
| to_update_in | n | 5/5 | 1.0 | 1028 | 14-15 | haiku/to_update_in-3 | PASS 947.9s (j0510) | none seen (cosmetic variants) |
| inbox_push | y | 5/5 | 1.0 | 258 | 8-9 | haiku/inbox_push-2 | PASS 92.2s (j0469) | none seen (cosmetic variants) |
| inbox_pop | y | 5/5 | 1.0 | 164 | 9-9 | haiku/inbox_pop-1 | PASS 98.6s (j0468) | none seen (cosmetic variants) |
| bot_http_result | y | 5/5 | 1.0 | 83 | 11-11 | haiku/bot_http_result-1 | PASS 51.0s (j0466) | none seen (cosmetic variants) |

## Haiku summary
- Overall: 120/125 passed (96%), mean runs 1.08, mean wall 186s, median wall 96s, mean body lines 18.3.
- Excluding on_update (harness OOM): 120/120 passed (100%), mean runs 1.01, mean wall 171s, median wall 79s, mean body lines 18.1.
- Leaf (95 builders): 95/95 passed (100%), mean runs 1.01, mean wall 138s, median wall 66s, mean body lines 17.7.
- Non-leaf (30 builders): 25/30 passed (83%), mean runs 1.30, mean wall 338s, median wall 158s, mean body lines 20.2.
- First-run passes: 119/125. Runs to pass (passers only): 1 run(s): 119, 2 run(s): 1.
- Total builder wall (sum): 6.5 h of builder time inside a ~56 min fleet wall.
- Audit: 124 of 125 branches clean (only logic/src/fns/<fn>.rs touched, nothing outside @body..@end changed, no unwind lowered, no bound loosened, no spec_* call in any body). One flag: decide-4 inserted a stray `// @end` inside its body, so the @body..@end region no longer ends at the real marker. It still compiled and proved, but the tooling that splices bodies would cut it short; fix or avoid before merging. Merge candidates are the other 4 decide branches.
- on_update: 0/5, every attempt OOM at MemoryMax=4G in the Kani stage with no verdict. Fix the harness/contract (smaller state, split, or stubs) before judging any model on it.

## Re-prove (audit)
One passing body per function (smallest unflagged), `proofq submit <wt> --who audit-<fn> --harness contract_<fn> --timeout 1800`, then `wait`, at most 3 in flight. First submissions were all served from proofq's result cache (identical sources, 'cached, proved by jNNNN', 0 s), so they proved nothing; the real re-proves were resubmitted with `--solver cadical` added to change the cache key. The `.rs` sources are unchanged.

| fn | branch | result | wall s | kani s | job |
|---|---|---|---|---|---|
| bot_http_result | haiku/bot_http_result-1 | PASS | 50.95 | 50.2 | j0466 |
| classify_key | haiku/classify_key-2 | PASS | 5.79 | 5.27 | j0465 |
| dec | haiku/dec-1 | PASS | 52.21 | 51.66 | j0464 |
| decide | haiku/decide-1 | PASS | 6.15 | 5.63 | j0467 |
| div10 | haiku/div10-1 | PASS | 14.33 | 13.94 | j0463 |
| inbox_pop | haiku/inbox_pop-1 | PASS | 98.62 | 98.18 | j0468 |
| inbox_push | haiku/inbox_push-2 | PASS | 92.22 | 91.6 | j0469 |
| interval_due | haiku/interval_due-1 | PASS | 1.94 | 1.48 | j0470 |
| judge_age | haiku/judge_age-1 | PASS | 56.47 | 56.01 | j0471 |
| lex_step | haiku/lex_step-4 | PASS | 3.69 | 3.23 | j0485 |
| make_anchor | haiku/make_anchor-1 | PASS | 46.76 | 46.31 | j0486 |
| name_from | haiku/name_from-4 | PASS | 5.96 | 5.33 | j0487 |
| on_poll | haiku/on_poll-1 | PASS | 362.89 | 362.51 | j0488 |
| op_budget_ms | haiku/op_budget_ms-1 | PASS | 2.32 | 1.93 | j0489 |
| parse_command | haiku/parse_command-3 | PASS | 35.95 | 35.57 | j0490 |
| parse_http_date | haiku/parse_http_date-5 | PASS | 34.88 | 34.36 | j0491 |
| parse_i64 | haiku/parse_i64-1 | PASS | 24.31 | 23.71 | j0492 |
| parse_u64 | haiku/parse_u64-1 | PASS | 317.89 | 27.2 | j0498 |
| pick_op | haiku/pick_op-1 | PASS | 343.45 | 39.01 | j0500 |
| scan_finish | haiku/scan_finish-2 | PASS | 66.67 | 2.28 | j0508 |
| tenths_of_minute | haiku/tenths_of_minute-1 | PASS | 238.88 | 200.15 | j0509 |
| to_update_in | haiku/to_update_in-3 | PASS | 947.93 | 947.52 | j0510 |
| uptime_s | haiku/uptime_s-1 | PASS | 63.15 | 45.48 | j0511 |
| whole_minutes | haiku/whole_minutes-1 | PASS | 206.67 | 206.2 | j0512 |

## Divergence note
Every passing body proves the same contract (`ensures == spec_<fn>`), so they agree wherever the harness covers. A quick read of the bodies (diffs of each k against k=1) found only cosmetic differences: variable names, let-bindings, formatting, `crate::` paths, `match` vs `if`, and pick_op-5 inlining interval_due. No obvious behavioural divergence, so no sign the contracts are too loose. Caveat: slice-taking functions are proved only for bounded input (classify_key k<=10, parse_command t<=10, name_from s<=34, parse_http_date s<=30, parse_i64/parse_u64 s<=21), so behaviour beyond those lengths is covered by reading, not proof. lex_step (54 lines, 5 variants) and parse_http_date were only skimmed.

## Finisher (2026-11-22)

**Body sources.** All 69 bodies are filled. 22 are fleet-1 Haiku bodies. They were taken as they were, and each passes its own contract. The other 47 are script bodies (`tools/fill_bodies.py`, which renders spec text into a body). on_poll and pick_op started as Haiku bodies, but those predate a spec change (the `last_good_poll`/`deaf` reboot), so their round-1 contracts failed. They were refilled by script in 77d023e. No new Haiku calls were made.

**Identity check.** The hash is sha256 of the body with whitespace removed. The "same as reference" column matches each body against the architect's reference proof crates (/tmp/gbx/j/*). 12 bodies match strictly: the fn file, spec.rs, lib.rs and props.rs are all unchanged. That makes them skippable under the 17:45 rule, but nothing was skipped. Every contract was proved again in the final pass below.

| function | body source | body sha256[:16] (ws-normalised) | same as architect reference proof | contract proof (this round) |
|---|---|---|---|---|
| boot_bound | script (spec text) | `4c5939a89ad3889b` | no | pass 186.94s j0910 |
| bot_feed | script (spec text) | `a65905c972be02bb` | yes, whole file + spec.rs + lib.rs (j0655) | pass 319.23s j0902 |
| bot_http_result | fleet-1 Haiku `haiku/bot_http_result-1` | `789a14d2cdf2aed1` | no | pass 62.93s j0920 |
| bot_next_update | script (spec text) | `27f49150f70ac920` | body only (j0641; spec.rs changed since) | pass 317.85s j0905 |
| bot_poll_done | script (spec text) | `1f57a2392d2c5288` | no | pass 165.41s j0909 |
| bot_send_done | script (spec text) | `fc4b9bd8140aa936` | body only (j0650; spec.rs changed since) | pass 82.71s j0917 |
| bot_step | script (spec text) | `e758e58735825052` | body only (j0633; spec.rs changed since) | pass 124.8s j0912 |
| classify_key | fleet-1 Haiku `haiku/classify_key-2` | `d0efd1753bbcf2b4` | no | pass 10.26s j0970 |
| close_timeout_due | script (spec text) | `686341b6cdfc362f` | no | pass 5.41s j0984 |
| closed_notice | script (spec text) | `3c4f21fdaf2eea18` | body only (j0390; spec.rs changed since) | pass 11.93s j0962 |
| deaf | script (spec text) | `57ffb90af3c69430` | no | pass 4.9s j0985 |
| dec | fleet-1 Haiku `haiku/dec-1` | `2c0993cf314fd8ad` | no | pass 52.83s j0921 |
| decide | fleet-1 Haiku `haiku/decide-1` | `e5d9a4adda80a1b2` | no | pass 6.19s j0981 |
| div10 | fleet-1 Haiku `haiku/div10-1` | `008dd77b5d56f18f` | body only (j0541; spec.rs changed since) | pass 44.18s j0926 |
| div60 | script (spec text) | `cc4241e04f393fa2` | body only (j0545; spec.rs changed since) | pass 108.54s j0914 |
| door_tick | script (spec text) | `5cbffc9a8c271162` | body only (j0495; spec.rs changed since) | pass 21.22s j0942 |
| inbox_pop | fleet-1 Haiku `haiku/inbox_pop-1` | `752ec350efa30da3` | no | pass 72.84s j0918 |
| inbox_push | fleet-1 Haiku `haiku/inbox_push-2` | `ccc1302dc13d9237` | no | pass 79.86s j0919 |
| interval_due | fleet-1 Haiku `haiku/interval_due-1` | `69aaddb9ccf5e4e6` | no | pass 4.78s j0983 |
| json_escape | script (spec text) | `9c01d3b0cc76f6a9` | no | pass 5.15s j0987 |
| judge_age | fleet-1 Haiku `haiku/judge_age-1` | `6feab0f9d88eea33` | no | pass 83.29s j0916 |
| lex_step | fleet-1 Haiku `haiku/lex_step-4` | `986d3b41ea4bd88d` | no | pass 8.83s j0979 |
| make_anchor | fleet-1 Haiku `haiku/make_anchor-1` | `496f5a1a55c02838` | no | pass 84.11s j0915 |
| name_from | fleet-1 Haiku `haiku/name_from-4` | `2a2ea95b3863bbf5` | no | pass 11.97s j0969 |
| on_poll | script (spec text) | `233c3c02292448ea` | no | pass 18.6s j0900 |
| on_update | script (spec text) | `f92d8d31029e58fc` | body only (j0609; spec.rs changed since) | pass 18.26s j0948 |
| op_budget_ms | fleet-1 Haiku `haiku/op_budget_ms-1` | `f57a108546aaa568` | no | pass 6.19s j0982 |
| op_done | script (spec text) | `ed34ef7184da9eb2` | no | pass 15.48s j0954 |
| open_alert_due | script (spec text) | `26790aa98d3ebe02` | no | pass 5.34s j0988 |
| overrun | script (spec text) | `44946f45c7860b99` | no | pass 6.67s j0990 |
| parse_command | fleet-1 Haiku `haiku/parse_command-3` | `2843a2a3c30ec677` | no | pass 49.15s j0928 |
| parse_http_date | fleet-1 Haiku `haiku/parse_http_date-5` | `88e975f598278c9e` | no | pass 34.6s j0929 |
| parse_i64 | fleet-1 Haiku `haiku/parse_i64-1` | `b374addd8143f9c7` | no | pass 18.23s j0946 |
| parse_u64 | fleet-1 Haiku `haiku/parse_u64-1` | `6cdf462dee1b9192` | no | pass 29.72s j0934 |
| pick_op | script (spec text) | `6fb1a4f9a47d153a` | no | pass 21.55s j0898 |
| push_ignored | script (spec text) | `3654bebce1636e82` | no | pass 21.13s j0950 |
| push_mention | script (spec text) | `c92fec31e786f132` | yes, whole file + spec.rs + lib.rs (j0643) | pass 27.13s j0940 |
| queue_door_events | script (spec text) | `b26ca9af610cf4db` | no | pass 9.15s j0966 |
| render_closed_after | script (spec text) | `1860ee7293a8bc1e` | yes, whole file + spec.rs + lib.rs (j0622) | pass 22.55s j0944 |
| render_diag | script (spec text) | `b2ee6a582d48ddbc` | no | pass 55.76s j0925 |
| render_heap | script (spec text) | `7770c5285ac6bfc2` | yes, whole file + spec.rs + lib.rs (j0652) | pass 21.05s j0943 |
| render_heartbeat | script (spec text) | `a649bd6088a709f0` | no | pass 31.97s j0935 |
| render_log | script (spec text) | `b2fde1116b898546` | yes, whole file + spec.rs + lib.rs (j0669) | pass 30.23s j0939 |
| render_msg | script (spec text) | `40d08b45f0e0139d` | yes, whole file + spec.rs + lib.rs (j0637) | pass 102.62s j0913 |
| render_poll_path | script (spec text) | `ece2db60ba5e8517` | yes, whole file + spec.rs + lib.rs (j0668) | pass 13.37s j0958 |
| render_reply | script (spec text) | `a2959d9da30ce0dd` | no | pass 248.43s j0906 |
| render_send_body | script (spec text) | `abc1bcf38993eae8` | yes, whole file + spec.rs + lib.rs (j0674) | pass 725.81s j0903 |
| scan_byte | script (spec text) | `939642a96ca7e110` | body only (j0507; spec.rs changed since) | pass 31.06s j0936 |
| scan_finish | fleet-1 Haiku `haiku/scan_finish-2` | `8f082c347d405c5a` | no | pass 4.78s j0989 |
| send_done | script (spec text) | `a6434b2e3cd9f4e9` | no | pass 12.45s j0957 |
| step | script (spec text) | `e9d8d625c3926633` | body only (j0611; spec.rs changed since) | pass 65.2s j0923 |
| tenths_of_minute | fleet-1 Haiku `haiku/tenths_of_minute-1` | `c993ef23752700b2` | no | pass 199.58s j0907 |
| text_push | script (spec text) | `ce66261710b659e0` | no | pass 784.62s j0897 |
| text_push_i64 | script (spec text) | `1a4869fbbab280a6` | yes, whole file + spec.rs + lib.rs (j0675) | pass 16.36s j0951 |
| text_push_json | script (spec text) | `0e5d22da49a8446b` | yes, whole file + spec.rs + lib.rs (j0707) | pass 338.89s j0904 |
| text_push_pad6 | script (spec text) | `4e064bc685a518ed` | yes, whole file + spec.rs + lib.rs (j0645) | pass 309.61s j0899 |
| text_push_u64 | script (spec text) | `bdb74657394f966b` | yes, whole file + spec.rs + lib.rs (j0493) | pass 27.96s j0938 |
| to_update_in | fleet-1 Haiku `haiku/to_update_in-3` | `979446c761775f2a` | no | pass 18.79s j0952 |
| uptime_s | fleet-1 Haiku `haiku/uptime_s-1` | `3208be9366d8c89b` | no | pass 60.9s j0924 |
| walk_close | script (spec text) | `f474384de0ca9b06` | no | pass 35.59s j0931 |
| walk_finish_scalar | script (spec text) | `b00349ff292620c6` | no | pass 44.78s j0927 |
| walk_open | script (spec text) | `6a2252011af65dbd` | no | pass 9.0s j0968 |
| walk_scalar | script (spec text) | `da5e8f66d3e215ef` | no | pass 35.09s j0933 |
| walk_step | script (spec text) | `aa95a517bbe06580` | no | pass 188.52s j0908 |
| walk_str_end | script (spec text) | `c4cf27209f9b76b9` | no | pass 51.63s j0930 |
| walk_str_start | script (spec text) | `6106216e6082510f` | no | pass 8.25s j0977 |
| walk_tmp_append | script (spec text) | `53e60e76efb6f32f` | no | pass 8.5s j0975 |
| whole_minutes | fleet-1 Haiku `haiku/whole_minutes-1` | `42df493679b9b0e0` | no | pass 713.26s j0901 |
| wifi_track | script (spec text) | `0ffb51c052fea16b` | no | pass 5.5s j0986 |

**Round 1** was run on d1f4ab6 (`~/wt/fin/results.jsonl`). 67/69 contracts passed; on_poll and pick_op failed (stale Haiku bodies, see above). 27/27 props/lemmas passed.

**Final full pass, no skips.** It ran on 77d023e; logic/ is identical at ac0e6ff. It used proofq `--who finisher*` with 8 worktrees, jobs j0897..j1040, and its results are in `~/wt/fin/final.jsonl`.
- 69/69 contracts, 27/27 props/lemmas: 96/96 sound.
- Merkle root over the sound proofs: `6338fb2a0410ec1114faa34e67a16a189a007c597066949a0bb4b2da67241581` (ROOT.txt).
- 44/48 verifier harnesses pass. 4 FAIL on their asserts. The bodies are not at fault: every contract proves body == spec, so these are claims about the spec.
  - v1_reply_within_two_passes (j0996), v1_poll_due_after_any_pause (j1000), v5_open_batch_makes_progress (j1005), v6_keeps_polling_when_polls_fail (j1038).
  - Common cause: `spec_pick_op` puts `spec_deaf(last_good_poll, now)` → Reboot ahead of every op except overrun. No poll has succeeded for DEAF_REBOOT_AFTER_MS, or never since boot with now >= that, and none of the four harnesses assumes otherwise.
  - So any_state() reaches the reboot, and Reboot is not a reply send, a Poll or a NextUpdate.
  - For v6 the reboot is arguably the intended answer to polls that keep failing; its assert forgot it.
  - **Fix the harnesses, not the core:** add `kani::assume(!spec_deaf(s.last_good_poll, now))` (v1/v5/v1.6), or allow `Op::Reboot` in v6. The owner should decide which.
  - The memory limit was not raised and no harness needed breaking down.

**Firmware** (ac0e6ff, `shell/`): esp-idf-svc 0.53 on IDF v5.3.3, `xtensa-esp32-espidf`. One loop, and the WDT is fed only at the top of each pass: one feed per completed `bot_step` + op. This fixes the main.py:120-160 bug, where it was fed during waits. WiFi, Telegram over HTTPS (cert bundle) and UDP logging are built in. Secrets come from env at build time (build.rs → `$OUT_DIR/secrets.rs`).
- A build with placeholder secrets succeeded: app 1,176,960 B (28.5% of 4 MB), merged image `shell/target/garagebot-merged.bin`.
- The source is copied to `garage-bot/firmware-rs/` and not committed there. It was not flashed.

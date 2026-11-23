//! GarageBot timing logic, ported from MicroPython (garage-bot/main.py v1.6.0)
//! and verified function-by-function with Kani contracts.
//!
//! Region markers (`// @fn`, `// @body`, `// @harness`, `// @end`) are read by
//! cache.py to compute per-harness proof keys. Keep them intact.
#![allow(clippy::all)]

// @prelude
/// MicroPython ESP32: MICROPY_PY_TIME_TICKS_PERIOD = MP_SMALL_INT_POSITIVE_MASK + 1
/// = 2^30 on a 32-bit build (31-bit small ints). ticks_ms() wraps every ~12.4 days.
pub const TICKS_PERIOD: u32 = 1 << 30;
pub const TICKS_HALF: u32 = TICKS_PERIOD / 2;
pub const TICKS_MASK: u32 = TICKS_PERIOD - 1;

/// Spec-only: true elapsed ms from `since` to `now`, modulo the period.
pub const fn spec_elapsed(now: u32, since: u32) -> u32 {
    (now + TICKS_PERIOD - since) % TICKS_PERIOD
}
// @end

// @fn ticks_diff
/// time.ticks_diff(end, start) — extmod/modtime.c:
/// ((end - start + PERIOD/2) & (PERIOD-1)) - PERIOD/2
#[cfg_attr(kani, kani::requires(end < TICKS_PERIOD && start < TICKS_PERIOD))]
#[cfg_attr(kani, kani::ensures(|r| *r >= -(TICKS_HALF as i32) && *r < TICKS_HALF as i32))]
#[cfg_attr(kani, kani::ensures(|r| {
    let e = spec_elapsed(end, start) as i64;
    *r as i64 == if e < TICKS_HALF as i64 { e } else { e - TICKS_PERIOD as i64 }
}))]
pub fn ticks_diff(end: u32, start: u32) -> i32 {
    // @body
    (end.wrapping_sub(start).wrapping_add(TICKS_HALF) & TICKS_MASK) as i32 - TICKS_HALF as i32
}
// @end

// @harness contract_ticks_diff target=ticks_diff
#[cfg(kani)]
#[kani::proof_for_contract(ticks_diff)]
fn contract_ticks_diff() {
    ticks_diff(kani::any(), kani::any());
}
// @end

// @fn ticks_add
/// Clock model: the hardware counter `t` after `d` real milliseconds.
/// (Also time.ticks_add.) Used by property harnesses to relate ticks to real time.
#[cfg_attr(kani, kani::requires(t < TICKS_PERIOD))]
#[cfg_attr(kani, kani::ensures(|r| *r < TICKS_PERIOD
    && *r as u64 == (t as u64 + d as u64) % TICKS_PERIOD as u64))]
pub fn ticks_add(t: u32, d: u32) -> u32 {
    // @body
    t.wrapping_add(d) & TICKS_MASK
}
// @end

// @harness contract_ticks_add target=ticks_add
#[cfg(kani)]
#[kani::proof_for_contract(ticks_add)]
fn contract_ticks_add() {
    ticks_add(kani::any(), kani::any());
}
// @end

// @harness prop_a_ticks_across_wrap stubs=ticks_diff,ticks_add
/// (a) For ANY counter value (so including every wrap position) and any real
/// elapsed time below half a period (~6.2 days), ticks_diff recovers the real
/// elapsed time exactly, and is antisymmetric.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(ticks_diff)]
#[kani::stub_verified(ticks_add)]
fn prop_a_ticks_across_wrap() {
    let start: u32 = kani::any();
    let d: u32 = kani::any();
    kani::assume(start < TICKS_PERIOD && d < TICKS_HALF);
    let now = ticks_add(start, d);
    assert!(ticks_diff(now, start) == d as i32);
    assert!(ticks_diff(start, now) == -(d as i32));
    kani::cover!(now < start, "the counter wrapped between start and now");
}
// @end

// @harness prop_a_wrap_hazard stubs=ticks_diff,ticks_add
/// (a, limit) Past half a period the diff goes NEGATIVE: real elapsed d in
/// [2^29, 2^30) ms reads as d - 2^30. Every `ticks_diff(..) >= X` test in
/// main.py silently turns false after ~6.2 days without a refresh.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(ticks_diff)]
#[kani::stub_verified(ticks_add)]
fn prop_a_wrap_hazard() {
    let start: u32 = kani::any();
    let d: u32 = kani::any();
    kani::assume(start < TICKS_PERIOD && d >= TICKS_HALF && d < TICKS_PERIOD);
    let now = ticks_add(start, d);
    let r = ticks_diff(now, start);
    assert!(r < 0 && r as i64 == d as i64 - TICKS_PERIOD as i64);
}
// @end

// @fn close_timeout_due
pub const CLOSE_TIMEOUT_SECONDS: u32 = 60;
pub const CLOSE_TIMEOUT_MS: u32 = CLOSE_TIMEOUT_SECONDS * 1000;

/// main.py:703  `time.ticks_diff(current_time, close_requested_time) >= CLOSE_TIMEOUT_SECONDS * 1000`
#[cfg_attr(kani, kani::requires(now < TICKS_PERIOD && requested < TICKS_PERIOD))]
#[cfg_attr(kani, kani::ensures(|r| {
    let e = spec_elapsed(now, requested);
    *r == (e >= CLOSE_TIMEOUT_MS && e < TICKS_HALF)
}))]
pub fn close_timeout_due(now: u32, requested: u32) -> bool {
    // @body
    ticks_diff(now, requested) >= (CLOSE_TIMEOUT_SECONDS * 1000) as i32
}
// @end

// @harness contract_close_timeout_due target=close_timeout_due stubs=ticks_diff
#[cfg(kani)]
#[kani::proof_for_contract(close_timeout_due)]
#[kani::stub_verified(ticks_diff)]
fn contract_close_timeout_due() {
    close_timeout_due(kani::any(), kani::any());
}
// @end

// @harness prop_b_due_iff_elapsed stubs=close_timeout_due,ticks_add
/// (b, timing core) due iff real elapsed time since the close request is AT LEAST
/// the timeout. (The literal "longer than" fails at exactly 60000 ms: main.py uses >=.)
/// Valid for real elapsed < 2^29 ms; see prop_b_missed_after_half_period.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(close_timeout_due)]
#[kani::stub_verified(ticks_add)]
fn prop_b_due_iff_elapsed() {
    let requested: u32 = kani::any();
    let d: u32 = kani::any();
    kani::assume(requested < TICKS_PERIOD && d < TICKS_HALF);
    let now = ticks_add(requested, d);
    assert!(close_timeout_due(now, requested) == (d >= CLOSE_TIMEOUT_MS));
}
// @end

// @fn close_check
/// Globals bot_triggered_close / close_requested_time / close_timeout_alerted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct CloseState {
    pub bot_triggered_close: bool,
    pub close_requested_time: Option<u32>,
    pub close_timeout_alerted: bool,
}
pub const CLOSE_IDLE: CloseState =
    CloseState { bot_triggered_close: false, close_requested_time: None, close_timeout_alerted: false };

pub const fn close_state_ok(st: &CloseState) -> bool {
    match st.close_requested_time { Some(t) => t < TICKS_PERIOD, None => true }
}
pub const fn close_armed(st: &CloseState) -> bool {
    st.bot_triggered_close && st.close_requested_time.is_some() && !st.close_timeout_alerted
}

/// main.py:701-707 (door open branch) and 715-721 (door closed branch), one loop pass.
/// Returns (new state, alert_fired). alert_fired == one "Door did not close!" send.
#[cfg_attr(kani, kani::requires(now < TICKS_PERIOD && close_state_ok(&st)))]
#[cfg_attr(kani, kani::ensures(|r| r.1 == (door_open && close_armed(&st) && {
    let e = spec_elapsed(now, match st.close_requested_time { Some(t) => t, None => 0 });
    e >= CLOSE_TIMEOUT_MS && e < TICKS_HALF
})))]
#[cfg_attr(kani, kani::ensures(|r| if door_open {
    r.0.bot_triggered_close == st.bot_triggered_close
        && r.0.close_requested_time == st.close_requested_time
        && r.0.close_timeout_alerted == (st.close_timeout_alerted || r.1)
} else {
    r.0 == CLOSE_IDLE
}))]
pub fn close_check(st: CloseState, door_open: bool, now: u32) -> (CloseState, bool) {
    // @body
    let mut st = st;
    let mut fired = false;
    if door_open {
        if st.bot_triggered_close && st.close_requested_time.is_some() && !st.close_timeout_alerted {
            if close_timeout_due(now, st.close_requested_time.unwrap()) {
                fired = true;
                st.close_timeout_alerted = true;
            }
        }
    } else {
        st.bot_triggered_close = false;
        st.close_requested_time = None;
        st.close_timeout_alerted = false;
    }
    (st, fired)
}
// @end

// @harness contract_close_check target=close_check stubs=close_timeout_due
#[cfg(kani)]
#[kani::proof_for_contract(close_check)]
#[kani::stub_verified(close_timeout_due)]
fn contract_close_check() {
    close_check(kani::any(), kani::any(), kani::any());
}
// @end

// @harness prop_b_alert_iff_open_past_timeout stubs=close_check,ticks_add
/// (b) After a bot-triggered close at tick `t0`, sampled once `d` real ms later
/// (d < 2^29): the alert fires iff the door is still open and d >= 60 s.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(close_check)]
#[kani::stub_verified(ticks_add)]
fn prop_b_alert_iff_open_past_timeout() {
    let t0: u32 = kani::any();
    let d: u32 = kani::any();
    let door_open: bool = kani::any();
    kani::assume(t0 < TICKS_PERIOD && d < TICKS_HALF);
    let armed = CloseState { bot_triggered_close: true, close_requested_time: Some(t0), close_timeout_alerted: false };
    let (st, fired) = close_check(armed, door_open, ticks_add(t0, d));
    assert!(fired == (door_open && d >= CLOSE_TIMEOUT_MS));
    assert!(!door_open || st.close_timeout_alerted == fired);
    assert!(door_open || st == CLOSE_IDLE);
}
// @end

// @harness prop_b_alert_at_most_once stubs=close_check
/// (b, exactly-once) Over any two later loop passes at arbitrary ticks, with the
/// door open throughout, a close request never produces a second alert.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(close_check)]
fn prop_b_alert_at_most_once() {
    let st0: CloseState = kani::any();
    let (n1, n2): (u32, u32) = (kani::any(), kani::any());
    kani::assume(close_state_ok(&st0) && n1 < TICKS_PERIOD && n2 < TICKS_PERIOD);
    let (st1, f1) = close_check(st0, true, n1);
    let (_st2, f2) = close_check(st1, true, n2);
    assert!(!(f1 && f2));
    assert!(!(st0.close_timeout_alerted && (f1 || f2)));
}
// @end

// @harness prop_b_missed_after_half_period stubs=close_check,ticks_add
/// (b, limit) If the first loop pass after a close request comes >= 2^29 ms
/// (~6.2 days) late, e.g. the loop sat in connect_wifi()'s infinite retry, the
/// alert does NOT fire although the door has been open far longer than 60 s.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(close_check)]
#[kani::stub_verified(ticks_add)]
fn prop_b_missed_after_half_period() {
    let t0: u32 = kani::any();
    let d: u32 = kani::any();
    kani::assume(t0 < TICKS_PERIOD && d >= TICKS_HALF && d < TICKS_PERIOD);
    let armed = CloseState { bot_triggered_close: true, close_requested_time: Some(t0), close_timeout_alerted: false };
    let (_st, fired) = close_check(armed, true, ticks_add(t0, d));
    assert!(!fired);
}
// @end

// @fn uptime_accumulate
pub const HEARTBEAT_INTERVAL_MS: u32 = 60 * 1000;
/// Largest number of seconds one heartbeat can add: (2^29 - 1) / 1000.
pub const UPTIME_MAX_STEP: u64 = ((TICKS_HALF - 1) / 1000) as u64;
/// Python ints do not overflow; the u64 port must stay below this to be faithful.
pub const UPTIME_SAFE_MAX: u64 = u64::MAX - UPTIME_MAX_STEP;

/// main.py:617-619. Returns (uptime_seconds', last_heartbeat_time', heartbeat_fired).
#[cfg_attr(kani, kani::requires(now < TICKS_PERIOD && last_hb < TICKS_PERIOD && uptime <= UPTIME_SAFE_MAX))]
#[cfg_attr(kani, kani::ensures(|r| {
    let e = spec_elapsed(now, last_hb);
    r.2 == (e >= HEARTBEAT_INTERVAL_MS && e < TICKS_HALF)
}))]
#[cfg_attr(kani, kani::ensures(|r| if r.2 {
    r.0 == uptime + (spec_elapsed(now, last_hb) / 1000) as u64 && r.1 == now
} else {
    r.0 == uptime && r.1 == last_hb
}))]
#[cfg_attr(kani, kani::ensures(|r| r.0 >= uptime && r.0 - uptime <= UPTIME_MAX_STEP))]
pub fn uptime_accumulate(uptime: u64, now: u32, last_hb: u32) -> (u64, u32, bool) {
    // @body
    let d = ticks_diff(now, last_hb);
    if d >= (60 * 1000) as i32 {
        (uptime + (d / 1000) as u64, now, true)
    } else {
        (uptime, last_hb, false)
    }
}
// @end

// @harness contract_uptime_accumulate target=uptime_accumulate stubs=ticks_diff
#[cfg(kani)]
#[kani::proof_for_contract(uptime_accumulate)]
#[kani::stub_verified(ticks_diff)]
fn contract_uptime_accumulate() {
    uptime_accumulate(kani::any(), kani::any(), kani::any());
}
// @end

// @harness prop_c_uptime_drift stubs=uptime_accumulate,ticks_add
/// (c, accuracy) FINDING: uptime_seconds is NOT real elapsed seconds. Each
/// heartbeat does `+= diff // 1000` and then `last_heartbeat_time = current_time`,
/// discarding diff % 1000 ms. Exact law over two heartbeats, plus a witness
/// that a whole second can already be lost after two heartbeats.
/// (The first attempt asserted u2 == (d1 + d2) / 1000 and failed.)
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(uptime_accumulate)]
#[kani::stub_verified(ticks_add)]
fn prop_c_uptime_drift() {
    let t0: u32 = kani::any();
    let (d1, d2): (u32, u32) = (kani::any(), kani::any());
    kani::assume(t0 < TICKS_PERIOD);
    kani::assume(d1 >= HEARTBEAT_INTERVAL_MS && d1 < 120_000);
    kani::assume(d2 >= HEARTBEAT_INTERVAL_MS && d2 < 120_000);
    let t1 = ticks_add(t0, d1);
    let (u1, hb1, f1) = uptime_accumulate(0, t1, t0);
    let t2 = ticks_add(t1, d2);
    let (u2, _hb2, f2) = uptime_accumulate(u1, t2, hb1);
    assert!(f1 && f2);
    assert!(u2 * 1000 + (d1 % 1000) as u64 + (d2 % 1000) as u64 == (d1 + d2) as u64);
    assert!(u2 <= ((d1 + d2) / 1000) as u64); // never runs fast
    kani::cover!(u2 + 1 == ((d1 + d2) / 1000) as u64, "a full second lost after two heartbeats");
}
// @end

// @harness prop_c_uptime_monotonic stubs=uptime_accumulate
/// (c) Monotonic: over any three loop passes with completely arbitrary tick
/// readings (any wrap, any stall), uptime never decreases and grows by at most
/// UPTIME_MAX_STEP per pass.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(uptime_accumulate)]
fn prop_c_uptime_monotonic() {
    let u0: u64 = kani::any();
    let hb0: u32 = kani::any();
    let n: [u32; 3] = kani::any();
    kani::assume(u0 <= UPTIME_SAFE_MAX - 2 * UPTIME_MAX_STEP && hb0 < TICKS_PERIOD);
    kani::assume(n[0] < TICKS_PERIOD && n[1] < TICKS_PERIOD && n[2] < TICKS_PERIOD);
    let (u1, hb1, _) = uptime_accumulate(u0, n[0], hb0);
    let (u2, hb2, _) = uptime_accumulate(u1, n[1], hb1);
    let (u3, _, _) = uptime_accumulate(u2, n[2], hb2);
    assert!(u0 <= u1 && u1 <= u2 && u2 <= u3);
    assert!(u3 - u0 <= 3 * UPTIME_MAX_STEP);
}
// @end

// @harness prop_c_uptime_no_overflow stubs=uptime_accumulate
/// (c) No overflow. Inductive step, multiplication-free: for any bound B that
/// leaves one step of headroom, uptime <= B implies the pass is defined (its
/// precondition holds, so the u64 add cannot overflow) and uptime' <= B + STEP.
/// By induction k passes from 0 give uptime <= k * UPTIME_MAX_STEP; the const
/// assertion below shows that needs more than 2^44 heartbeats (>= 60 s each,
/// i.e. tens of millions of years) to reach UPTIME_SAFE_MAX.
/// (First version put k * UPTIME_MAX_STEP inside the harness: 64-bit multiply
/// in SAT, killed after 2 minutes without an answer.)
const _: () = {
    if UPTIME_SAFE_MAX / UPTIME_MAX_STEP < (1 << 44) {
        panic!("uptime headroom too small");
    }
};
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(uptime_accumulate)]
fn prop_c_uptime_no_overflow() {
    let bound: u64 = kani::any();
    let u: u64 = kani::any();
    let (now, hb): (u32, u32) = (kani::any(), kani::any());
    kani::assume(bound <= UPTIME_SAFE_MAX - UPTIME_MAX_STEP && u <= bound);
    kani::assume(now < TICKS_PERIOD && hb < TICKS_PERIOD);
    let (u1, _, _) = uptime_accumulate(u, now, hb);
    assert!(u1 <= bound + UPTIME_MAX_STEP);
    assert!(u1 <= UPTIME_SAFE_MAX);
}
// @end

// @fn uptime_accumulate_fixed
/// Suggested fix (not in main.py): advance last_heartbeat_time by the whole
/// seconds counted, keeping the remainder for the next heartbeat.
#[cfg_attr(kani, kani::requires(now < TICKS_PERIOD && last_hb < TICKS_PERIOD && uptime <= UPTIME_SAFE_MAX))]
#[cfg_attr(kani, kani::ensures(|r| {
    let e = spec_elapsed(now, last_hb);
    r.2 == (e >= HEARTBEAT_INTERVAL_MS && e < TICKS_HALF)
}))]
#[cfg_attr(kani, kani::ensures(|r| if r.2 {
    let e = spec_elapsed(now, last_hb);
    r.0 == uptime + (e / 1000) as u64 && r.1 < TICKS_PERIOD && spec_elapsed(now, r.1) == e % 1000
} else {
    r.0 == uptime && r.1 == last_hb
}))]
pub fn uptime_accumulate_fixed(uptime: u64, now: u32, last_hb: u32) -> (u64, u32, bool) {
    // @body
    let d = ticks_diff(now, last_hb);
    if d >= (60 * 1000) as i32 {
        let secs = (d / 1000) as u32;
        (uptime + secs as u64, ticks_add(last_hb, secs * 1000), true)
    } else {
        (uptime, last_hb, false)
    }
}
// @end

// @harness contract_uptime_accumulate_fixed target=uptime_accumulate_fixed stubs=ticks_diff,ticks_add
#[cfg(kani)]
#[kani::proof_for_contract(uptime_accumulate_fixed)]
#[kani::stub_verified(ticks_diff)]
#[kani::stub_verified(ticks_add)]
fn contract_uptime_accumulate_fixed() {
    uptime_accumulate_fixed(kani::any(), kani::any(), kani::any());
}
// @end

// @harness prop_c_fixed_no_drift stubs=uptime_accumulate_fixed,ticks_add
/// With the fix, two heartbeats count exactly the real elapsed whole seconds.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(uptime_accumulate_fixed)]
#[kani::stub_verified(ticks_add)]
fn prop_c_fixed_no_drift() {
    let t0: u32 = kani::any();
    let (d1, d2): (u32, u32) = (kani::any(), kani::any());
    kani::assume(t0 < TICKS_PERIOD);
    kani::assume(d1 >= HEARTBEAT_INTERVAL_MS && d1 < 120_000);
    kani::assume(d2 >= HEARTBEAT_INTERVAL_MS && d2 < 120_000);
    let t1 = ticks_add(t0, d1);
    let (u1, hb1, f1) = uptime_accumulate_fixed(0, t1, t0);
    let t2 = ticks_add(t1, d2);
    let (u2, _hb2, f2) = uptime_accumulate_fixed(u1, t2, hb1);
    assert!(f1 && f2);
    assert!(u2 == ((d1 + d2) / 1000) as u64);
}
// @end

// @fn stale_command_age
/// Abstraction of the command text after `.lower().strip().split("@")[0]`.
/// main.py's handler accepts exactly "x" and "/x"; the stale guard strips leading
/// "/" and matches open/close/update. So every text the handler acts on as
/// Open/Close/Update is also guarded (checked by reading, not by Kani).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Cmd { Status, Open, Close, Silence, Update, Debug, Unknown }

pub const MAX_COMMAND_AGE_SECONDS: i64 = 60;
/// Bound on unix timestamps so the i64 port matches Python's unbounded ints.
pub const UNIX_MAX: i64 = 1 << 40;

/// STALE_GUARDED_COMMANDS = ["open", "close", "update"]
pub const fn guarded(cmd: Cmd) -> bool {
    matches!(cmd, Cmd::Open | Cmd::Close | Cmd::Update)
}
/// Spec: the message is stale. `date == 0` models a missing/zero "date" field.
pub const fn spec_stale(clock_synced: bool, cmd: Cmd, date: i64, now_unix: i64) -> bool {
    clock_synced && guarded(cmd) && date != 0 && now_unix - date > MAX_COMMAND_AGE_SECONDS
}

/// main.py:455-465. Some(age) if this is a guarded command that is too old.
#[cfg_attr(kani, kani::requires(date >= 0 && date <= UNIX_MAX && now_unix >= 0 && now_unix <= UNIX_MAX))]
#[cfg_attr(kani, kani::ensures(|r| r.is_some() == spec_stale(clock_synced, cmd, date, now_unix)))]
#[cfg_attr(kani, kani::ensures(|r| match *r { Some(age) => age == now_unix - date && age > MAX_COMMAND_AGE_SECONDS, None => true }))]
pub fn stale_command_age(clock_synced: bool, cmd: Cmd, date: i64, now_unix: i64) -> Option<i64> {
    // @body
    if !clock_synced {
        return None;
    }
    if !matches!(cmd, Cmd::Open | Cmd::Close | Cmd::Update) {
        return None;
    }
    if date == 0 {
        return None;
    }
    let age = now_unix - date;
    if age > 60 { Some(age) } else { None }
}
// @end

// @harness contract_stale_command_age target=stale_command_age
#[cfg(kani)]
#[kani::proof_for_contract(stale_command_age)]
fn contract_stale_command_age() {
    stale_command_age(kani::any(), kani::any(), kani::any(), kani::any());
}
// @end

// @harness prop_stale_filter stubs=stale_command_age
/// Stale filtering: with a synced clock, a dated door-actuating command is
/// refused iff it is MORE than 60 s old (exactly 60 s still runs); a command
/// dated in the future is never stale; non-actuating commands are never stale.
/// Fail-open cases, by design in main.py: clock not synced, or no date field.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(stale_command_age)]
fn prop_stale_filter() {
    let (synced, cmd): (bool, Cmd) = (kani::any(), kani::any());
    let (date, now): (i64, i64) = (kani::any(), kani::any());
    kani::assume(date >= 0 && date <= UNIX_MAX && now >= 0 && now <= UNIX_MAX);
    let r = stale_command_age(synced, cmd, date, now);
    if synced && guarded(cmd) && date != 0 {
        assert!(r.is_some() == (now - date > 60));
        assert!(now >= date || r.is_none());
    }
    if !guarded(cmd) || !synced || date == 0 {
        assert!(r.is_none());
    }
    kani::cover!(r.is_none() && guarded(cmd) && !synced && now - date > 86_000, "fail-open: day-old /open runs when NTP never synced");
}
// @end

// @fn send_telegram
/// How one urequests call ends (main.py:341-369).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum NetOutcome { Ok200, OkOtherStatus, OsError, OtherError }

/// http_ok / http_fail / http_sock_err. ok + fail == number of HTTP attempts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Http { pub ok: u32, pub fail: u32, pub sock_err: u32 }

/// Watchdog ghost state: ms since the last wdt.feed(), and the worst gap seen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Wdt { pub gap: u32, pub max_gap: u32 }

pub const WATCHDOG_TIMEOUT_MS: u32 = 300_000;
/// ASSUMPTION (not proven): one blocking HTTP call returns within this budget.
/// main.py asks for timeout=10 (per socket op); the proofs only need < 299 s.
pub const HTTP_CALL_MAX_MS: u32 = WATCHDOG_TIMEOUT_MS - 1000;
pub const HTTP_COUNTER_MAX: u32 = u32::MAX - 64;

pub const fn http_attempts(h: &Http) -> u64 { h.ok as u64 + h.fail as u64 }
pub const fn wdt_ok(w: &Wdt) -> bool { w.gap <= w.max_gap }
pub const fn max_u32(a: u32, b: u32) -> u32 { if a > b { a } else { b } }

/// send_telegram_message: feed the watchdog, ONE post attempt, never raises,
/// never retries. Returns (counters, watchdog, status == 200).
#[cfg_attr(kani, kani::requires(dur_ms <= HTTP_CALL_MAX_MS && wdt_ok(&wdt)
    && http.ok < HTTP_COUNTER_MAX && http.fail < HTTP_COUNTER_MAX && http.sock_err < HTTP_COUNTER_MAX))]
#[cfg_attr(kani, kani::ensures(|r| http_attempts(&r.0) == http_attempts(&http) + 1))]
#[cfg_attr(kani, kani::ensures(|r| r.2 == matches!(out, NetOutcome::Ok200)
    && r.0.ok == http.ok + matches!(out, NetOutcome::Ok200 | NetOutcome::OkOtherStatus) as u32
    && r.0.sock_err == http.sock_err + matches!(out, NetOutcome::OsError) as u32))]
#[cfg_attr(kani, kani::ensures(|r| r.1.gap == dur_ms && r.1.max_gap == max_u32(wdt.max_gap, dur_ms)))]
pub fn send_telegram(http: Http, wdt: Wdt, out: NetOutcome, dur_ms: u32) -> (Http, Wdt, bool) {
    // @body
    let mut http = http;
    let mut wdt = wdt;
    wdt.gap = 0; // wdt.feed()
    wdt.gap += dur_ms; // the blocking post
    if wdt.gap > wdt.max_gap {
        wdt.max_gap = wdt.gap;
    }
    match out {
        NetOutcome::Ok200 => { http.ok += 1; (http, wdt, true) }
        NetOutcome::OkOtherStatus => { http.ok += 1; (http, wdt, false) }
        NetOutcome::OsError => { http.fail += 1; http.sock_err += 1; (http, wdt, false) }
        NetOutcome::OtherError => { http.fail += 1; (http, wdt, false) }
    }
}
// @end

// @harness contract_send_telegram target=send_telegram
#[cfg(kani)]
#[kani::proof_for_contract(send_telegram)]
fn contract_send_telegram() {
    send_telegram(kani::any(), kani::any(), kani::any(), kani::any());
}
// @end

// @fn handle_command
/// What handle_command returns: None, a text to send, or the "SILENCE" marker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Reply { None, Text, Silence }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct HandlerOut {
    pub reply: Reply,
    /// press_garage_button() ran (relay pulse, 0.5 s blocking)
    pub pressed: bool,
    pub close: CloseState,
    /// update found: handler sends 2 notices itself, then do_ota_update() (not modelled further)
    pub ota: bool,
}

/// main.py:487-542. `now` is the fresh time.ticks_ms() read by the close branch.
#[cfg_attr(kani, kani::requires(now < TICKS_PERIOD && close_state_ok(&close)))]
#[cfg_attr(kani, kani::ensures(|r| r.pressed == (matches!(cmd, Cmd::Open) && !door_open || matches!(cmd, Cmd::Close) && door_open)))]
#[cfg_attr(kani, kani::ensures(|r| r.ota == (matches!(cmd, Cmd::Update) && update_available)))]
#[cfg_attr(kani, kani::ensures(|r| r.reply == match cmd {
    Cmd::Unknown => Reply::None,
    Cmd::Silence => Reply::Silence,
    Cmd::Update => if update_available { Reply::None } else { Reply::Text },
    _ => Reply::Text,
}))]
#[cfg_attr(kani, kani::ensures(|r| close_state_ok(&r.close) && r.close == if matches!(cmd, Cmd::Close) && door_open {
    CloseState { bot_triggered_close: true, close_requested_time: Some(now), close_timeout_alerted: false }
} else {
    close
}))]
pub fn handle_command(cmd: Cmd, door_open: bool, close: CloseState, now: u32, update_available: bool) -> HandlerOut {
    // @body
    let mut o = HandlerOut { reply: Reply::None, pressed: false, close, ota: false };
    match cmd {
        Cmd::Status => o.reply = Reply::Text,
        Cmd::Open => {
            if !door_open {
                o.pressed = true;
            }
            o.reply = Reply::Text;
        }
        Cmd::Close => {
            if door_open {
                o.close.bot_triggered_close = true;
                o.close.close_requested_time = Some(now);
                o.close.close_timeout_alerted = false;
                o.pressed = true;
            }
            o.reply = Reply::Text;
        }
        Cmd::Silence => o.reply = Reply::Silence,
        Cmd::Update => {
            if update_available {
                o.ota = true;
            } else {
                o.reply = Reply::Text;
            }
        }
        Cmd::Debug => o.reply = Reply::Text,
        Cmd::Unknown => {}
    }
    o
}
// @end

// @harness contract_handle_command target=handle_command
#[cfg(kani)]
#[kani::proof_for_contract(handle_command)]
fn contract_handle_command() {
    handle_command(kani::any(), kani::any(), kani::any(), kani::any(), kani::any());
}
// @end

// @fn process_update
/// One element of getUpdates' result, abstracted to what the loop body reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Update {
    pub update_id: u32,
    /// "message" in update and "text" in update["message"]
    pub has_text: bool,
    /// str(chat_id) == str(TELEGRAM_CHAT_ID)
    pub authorized: bool,
    pub cmd: Cmd,
    /// message["date"], 0 if absent
    pub date: i64,
}

/// Everything the world decides while one update is processed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Env {
    pub clock_synced: bool,
    pub now_unix: i64,
    pub door_open: bool,
    pub now_ticks: u32,
    /// check_for_update() found a newer version
    pub update_available: bool,
    /// outcome / duration of the 1st and 2nd send made for this update
    pub out: [NetOutcome; 2],
    pub dur: [u32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct BotState {
    pub last_update_id: u32,
    pub http: Http,
    pub wdt: Wdt,
    pub close: CloseState,
    pub muted: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct UpdResult {
    /// send_telegram_message attempts made for this update
    pub attempts: u8,
    pub pressed: bool,
    pub ota: bool,
}

pub const PRESS_MS: u32 = 500;
pub const GAP_SANE: u32 = 1 << 30;

pub const fn env_ok(e: &Env) -> bool {
    e.now_unix >= 0 && e.now_unix <= UNIX_MAX && e.now_ticks < TICKS_PERIOD
        && e.dur[0] <= HTTP_CALL_MAX_MS && e.dur[1] <= HTTP_CALL_MAX_MS
}
pub const fn update_ok(u: &Update) -> bool { u.date >= 0 && u.date <= UNIX_MAX }
/// State is well formed and the counters can take `headroom` more HTTP calls.
pub const fn bot_ok(st: &BotState, headroom: u32) -> bool {
    wdt_ok(&st.wdt) && st.wdt.max_gap <= GAP_SANE && close_state_ok(&st.close)
        && st.http.ok < HTTP_COUNTER_MAX - headroom
        && st.http.fail < HTTP_COUNTER_MAX - headroom
        && st.http.sock_err < HTTP_COUNTER_MAX - headroom
}
/// A command from the authorised chat.
pub const fn spec_actionable(u: &Update) -> bool { u.has_text && u.authorized }
pub const fn spec_is_stale(u: &Update, e: &Env) -> bool {
    spec_actionable(u) && spec_stale(e.clock_synced, u.cmd, u.date, e.now_unix)
}
pub const fn spec_is_fresh(u: &Update, e: &Env) -> bool {
    spec_actionable(u) && !spec_stale(e.clock_synced, u.cmd, u.date, e.now_unix)
}
/// Number of send attempts main.py makes for this update.
pub const fn spec_attempts(u: &Update, e: &Env) -> u8 {
    if !spec_actionable(u) { 0 }
    else if spec_is_stale(u, e) { 1 } // the "Ignored '<cmd>' ..." notice
    else {
        match u.cmd {
            Cmd::Unknown => 0,
            Cmd::Update => if e.update_available { 2 } else { 1 },
            _ => 1,
        }
    }
}
pub const fn spec_pressed(u: &Update, e: &Env) -> bool {
    spec_is_fresh(u, e) && (matches!(u.cmd, Cmd::Open) && !e.door_open || matches!(u.cmd, Cmd::Close) && e.door_open)
}

/// Body of `for update in updates:` (main.py:643-667).
#[cfg_attr(kani, kani::requires(bot_ok(&st, 2) && update_ok(&upd) && env_ok(&env)))]
#[cfg_attr(kani, kani::ensures(|r| r.0.last_update_id == upd.update_id))]
#[cfg_attr(kani, kani::ensures(|r| r.1.attempts == spec_attempts(&upd, &env)
    && http_attempts(&r.0.http) == http_attempts(&st.http) + r.1.attempts as u64
    && r.0.http.ok >= st.http.ok && r.0.http.ok - st.http.ok <= r.1.attempts as u32
    && r.0.http.fail >= st.http.fail && r.0.http.fail - st.http.fail <= r.1.attempts as u32
    && r.0.http.sock_err >= st.http.sock_err && r.0.http.sock_err - st.http.sock_err <= r.1.attempts as u32))]
#[cfg_attr(kani, kani::ensures(|r| r.1.pressed == spec_pressed(&upd, &env)
    && r.1.ota == (spec_is_fresh(&upd, &env) && matches!(upd.cmd, Cmd::Update) && env.update_available)))]
#[cfg_attr(kani, kani::ensures(|r| r.0.muted == (st.muted || spec_is_fresh(&upd, &env) && matches!(upd.cmd, Cmd::Silence))))]
#[cfg_attr(kani, kani::ensures(|r| close_state_ok(&r.0.close) && r.0.close == if spec_pressed(&upd, &env) && matches!(upd.cmd, Cmd::Close) {
    CloseState { bot_triggered_close: true, close_requested_time: Some(env.now_ticks), close_timeout_alerted: false }
} else {
    st.close
}))]
#[cfg_attr(kani, kani::ensures(|r| wdt_ok(&r.0.wdt)
    && r.0.wdt.gap <= max_u32(st.wdt.gap, HTTP_CALL_MAX_MS)
    && r.0.wdt.max_gap <= max_u32(st.wdt.max_gap, max_u32(st.wdt.gap + PRESS_MS, HTTP_CALL_MAX_MS))))]
pub fn process_update(st: BotState, upd: Update, env: Env) -> (BotState, UpdResult) {
    // @body
    let mut st = st;
    let mut res = UpdResult { attempts: 0, pressed: false, ota: false };
    st.last_update_id = upd.update_id;
    if upd.has_text {
        if upd.authorized {
            let age = stale_command_age(env.clock_synced, upd.cmd, upd.date, env.now_unix);
            if age.is_some() {
                let (h, w, _) = send_telegram(st.http, st.wdt, env.out[0], env.dur[0]);
                st.http = h;
                st.wdt = w;
                res.attempts += 1;
                return (st, res); // `continue`
            }
            let ho = handle_command(upd.cmd, env.door_open, st.close, env.now_ticks, env.update_available);
            st.close = ho.close;
            res.pressed = ho.pressed;
            res.ota = ho.ota;
            if ho.pressed {
                st.wdt.gap += PRESS_MS; // time.sleep(0.5) inside press_garage_button
                if st.wdt.gap > st.wdt.max_gap {
                    st.wdt.max_gap = st.wdt.gap;
                }
            }
            if ho.ota {
                // "Update available" + "Installing update now..." then do_ota_update()
                let (h, w, _) = send_telegram(st.http, st.wdt, env.out[0], env.dur[0]);
                let (h, w, _) = send_telegram(h, w, env.out[1], env.dur[1]);
                st.http = h;
                st.wdt = w;
                res.attempts += 2;
            }
            match ho.reply {
                Reply::Silence => {
                    st.muted = true;
                    let (h, w, _) = send_telegram(st.http, st.wdt, env.out[0], env.dur[0]);
                    st.http = h;
                    st.wdt = w;
                    res.attempts += 1;
                }
                Reply::Text => {
                    let (h, w, _) = send_telegram(st.http, st.wdt, env.out[0], env.dur[0]);
                    st.http = h;
                    st.wdt = w;
                    res.attempts += 1;
                }
                Reply::None => {}
            }
        }
    }
    (st, res)
}
// @end

// @harness contract_process_update target=process_update stubs=stale_command_age,send_telegram,handle_command
#[cfg(kani)]
#[kani::proof_for_contract(process_update)]
#[kani::stub_verified(stale_command_age)]
#[kani::stub_verified(send_telegram)]
#[kani::stub_verified(handle_command)]
fn contract_process_update() {
    process_update(kani::any(), kani::any(), kani::any());
}
// @end

// @harness prop_d_per_command stubs=process_update
/// (d), per update, for ANY network outcome and duration of each send:
///  * fresh command with a reply (status/open/close/silence/debug, or update
///    with nothing to install): exactly ONE send attempt, never retried;
///  * stale command: NOT executed (no relay press, no close arming, no OTA, no
///    mute) and exactly one "Ignored ..." notice;
///  * unknown text, other chats, non-text updates: nothing sent, nothing done;
///  * update with a new version: two notices, then the OTA path (not modelled).
/// The literal property ("fresh => exactly one, stale => none") failed on both
/// halves: see LOG.md.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(process_update)]
fn prop_d_per_command() {
    let (st, upd, env): (BotState, Update, Env) = (kani::any(), kani::any(), kani::any());
    kani::assume(bot_ok(&st, 2) && update_ok(&upd) && env_ok(&env));
    let (st1, r) = process_update(st, upd, env);
    let fresh = spec_is_fresh(&upd, &env);
    let stale = spec_is_stale(&upd, &env);
    let ota = matches!(upd.cmd, Cmd::Update) && env.update_available;
    if fresh && !matches!(upd.cmd, Cmd::Unknown) && !ota {
        assert!(r.attempts == 1);
    }
    if stale {
        assert!(r.attempts == 1 && !r.pressed && !r.ota);
        assert!(st1.close == st.close && st1.muted == st.muted);
    }
    if !spec_actionable(&upd) || (fresh && matches!(upd.cmd, Cmd::Unknown)) {
        assert!(r.attempts == 0 && !r.pressed && !r.ota && st1.close == st.close && st1.http == st.http);
    }
    if fresh && ota {
        assert!(r.attempts == 2 && r.ota && !r.pressed);
    }
    // the relay only ever moves for a fresh open/close, at most once per update
    assert!(!r.pressed || (fresh && matches!(upd.cmd, Cmd::Open | Cmd::Close)));
    // attempts are attempts: counted whether they succeed, time out or raise
    assert!(http_attempts(&st1.http) == http_attempts(&st.http) + r.attempts as u64);
    assert!(st1.last_update_id == upd.update_id);
    kani::cover!(fresh && r.attempts == 1 && matches!(env.out[0], NetOutcome::OsError), "one attempt on timeout");
    kani::cover!(stale && matches!(upd.cmd, Cmd::Open) && !env.door_open, "stale /open not executed");
}
// @end

// @fn poll_step
pub const MAX_UPDATES: usize = 3;

/// Inputs of one "Poll Telegram for commands" block (main.py:634-667).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct PollIn {
    /// backlog_flushed on entry
    pub flushed: bool,
    /// outcome of the getUpdates GET (flush or poll). Ok200 = JSON with ok:true;
    /// OkOtherStatus = a JSON body with ok:false; the rest raise.
    pub net: NetOutcome,
    pub dur: u32,
    /// flush only: update_id of the newest queued update, if any
    pub flush_newest: Option<u32>,
    /// poll only: what Telegram would return
    pub n: usize,
    pub updates: [Update; MAX_UPDATES],
    pub envs: [Env; MAX_UPDATES],
    /// if an update triggers OTA, the download succeeds and machine.reset() runs
    pub ota_reboot: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct PollOut {
    pub flushed: bool,
    /// updates whose loop body ran
    pub processed: usize,
    pub attempts: [u8; MAX_UPDATES],
    pub pressed: [bool; MAX_UPDATES],
    pub rebooted: bool,
}

pub const fn poll_in_ok(p: &PollIn) -> bool {
    p.n <= MAX_UPDATES && p.dur <= HTTP_CALL_MAX_MS
        && update_ok(&p.updates[0]) && update_ok(&p.updates[1]) && update_ok(&p.updates[2])
        && env_ok(&p.envs[0]) && env_ok(&p.envs[1]) && env_ok(&p.envs[2])
}
pub const fn spec_ota(p: &PollIn, i: usize) -> bool {
    spec_is_fresh(&p.updates[i], &p.envs[i]) && matches!(p.updates[i].cmd, Cmd::Update) && p.envs[i].update_available
}
/// Updates are delivered to the loop iff the backlog was flushed and the GET worked.
pub const fn spec_delivered(p: &PollIn) -> bool { p.flushed && matches!(p.net, NetOutcome::Ok200) }
pub const fn spec_processed(p: &PollIn) -> usize {
    if !spec_delivered(p) { 0 }
    else if p.n >= 1 && p.ota_reboot && spec_ota(p, 0) { 1 }
    else if p.n >= 2 && p.ota_reboot && spec_ota(p, 1) { 2 }
    else { p.n }
}
pub const fn spec_item(p: &PollIn, o: &PollOut, i: usize) -> bool {
    if i < o.processed {
        o.attempts[i] == spec_attempts(&p.updates[i], &p.envs[i]) && o.pressed[i] == spec_pressed(&p.updates[i], &p.envs[i])
    } else {
        o.attempts[i] == 0 && !o.pressed[i]
    }
}

#[cfg_attr(kani, kani::requires(bot_ok(&st, 8) && poll_in_ok(&pin)))]
#[cfg_attr(kani, kani::ensures(|r| r.1.processed == spec_processed(&pin)
    && spec_item(&pin, &r.1, 0) && spec_item(&pin, &r.1, 1) && spec_item(&pin, &r.1, 2)))]
#[cfg_attr(kani, kani::ensures(|r| r.1.rebooted == (pin.ota_reboot && r.1.processed > 0 && spec_ota(&pin, r.1.processed - 1))))]
#[cfg_attr(kani, kani::ensures(|r| r.1.flushed == (pin.flushed || matches!(pin.net, NetOutcome::Ok200))))]
#[cfg_attr(kani, kani::ensures(|r| r.0.last_update_id == if r.1.processed > 0 {
    pin.updates[r.1.processed - 1].update_id
} else if !pin.flushed && matches!(pin.net, NetOutcome::Ok200) {
    match pin.flush_newest { Some(id) => id, None => st.last_update_id }
} else {
    st.last_update_id
}))]
#[cfg_attr(kani, kani::ensures(|r| http_attempts(&r.0.http)
    == http_attempts(&st.http) + 1 + r.1.attempts[0] as u64 + r.1.attempts[1] as u64 + r.1.attempts[2] as u64))]
#[cfg_attr(kani, kani::ensures(|r| bot_ok(&r.0, 0)
    && r.0.wdt.gap <= HTTP_CALL_MAX_MS
    && r.0.wdt.max_gap <= max_u32(st.wdt.max_gap, HTTP_CALL_MAX_MS + PRESS_MS)))]
pub fn poll_step(st: BotState, pin: PollIn) -> (BotState, PollOut) {
    // @body
    let mut st = st;
    let mut out = PollOut {
        flushed: pin.flushed,
        processed: 0,
        attempts: [0; MAX_UPDATES],
        pressed: [false; MAX_UPDATES],
        rebooted: false,
    };
    // both flush_pending_updates() and get_telegram_updates(): wdt.feed(), one GET
    st.wdt.gap = pin.dur;
    if st.wdt.gap > st.wdt.max_gap {
        st.wdt.max_gap = st.wdt.gap;
    }
    match pin.net {
        NetOutcome::Ok200 | NetOutcome::OkOtherStatus => st.http.ok += 1,
        NetOutcome::OsError => { st.http.fail += 1; st.http.sock_err += 1; }
        NetOutcome::OtherError => st.http.fail += 1,
    }
    let got = matches!(pin.net, NetOutcome::Ok200);
    let n = if !pin.flushed {
        // Never execute commands until the offline backlog has been discarded
        if got {
            out.flushed = true;
            if let Some(id) = pin.flush_newest {
                st.last_update_id = id;
            }
        }
        0 // updates = []
    } else if got {
        pin.n
    } else {
        0 // poll failed: return []
    };
    let mut i = 0;
    while i < n {
        let (s, r) = process_update(st, pin.updates[i], pin.envs[i]);
        st = s;
        out.attempts[i] = r.attempts;
        out.pressed[i] = r.pressed;
        out.processed = i + 1;
        if r.ota && pin.ota_reboot {
            out.rebooted = true; // machine.reset(): the rest of the batch is never looked at
            break;
        }
        i += 1;
    }
    (st, out)
}
// @end

// @harness contract_poll_step target=poll_step stubs=process_update
#[cfg(kani)]
#[kani::proof_for_contract(poll_step)]
#[kani::stub_verified(process_update)]
#[kani::unwind(5)]
fn contract_poll_step() {
    poll_step(kani::any(), kani::any());
}
// @end

// @harness prop_d_poll_batch stubs=poll_step
/// (d), whole poll, for ANY batch of up to 3 updates and ANY mix of network
/// success / timeout / error on the GET and on every send:
///  * each delivered fresh replying command gets exactly one send attempt;
///  * each delivered stale command is not executed (one notice);
///  * every delivered update is processed unless an OTA reboot cuts the batch;
///  * if the GET fails nothing is processed and LAST_UPDATE_ID is untouched,
///    so the same updates are requested again next poll (no loss, no repeat);
///  * HTTP attempts == 1 GET + the per-update attempts, nothing hidden.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(poll_step)]
#[kani::unwind(5)]
fn prop_d_poll_batch() {
    let (st, pin): (BotState, PollIn) = (kani::any(), kani::any());
    kani::assume(bot_ok(&st, 8) && poll_in_ok(&pin));
    let (st1, out) = poll_step(st, pin);
    let mut i = 0;
    while i < MAX_UPDATES {
        let (u, e) = (&pin.updates[i], &pin.envs[i]);
        if i < out.processed {
            let ota = matches!(u.cmd, Cmd::Update) && e.update_available;
            if spec_is_fresh(u, e) && !matches!(u.cmd, Cmd::Unknown) && !ota {
                assert!(out.attempts[i] == 1);
            }
            if spec_is_stale(u, e) {
                assert!(out.attempts[i] == 1 && !out.pressed[i]);
            }
        } else {
            assert!(out.attempts[i] == 0 && !out.pressed[i]);
        }
        i += 1;
    }
    if spec_delivered(&pin) {
        assert!(out.rebooted || out.processed == pin.n);
    } else {
        assert!(out.processed == 0);
        if pin.flushed {
            assert!(st1.last_update_id == st.last_update_id);
        }
    }
    assert!(http_attempts(&st1.http)
        == http_attempts(&st.http) + 1 + out.attempts[0] as u64 + out.attempts[1] as u64 + out.attempts[2] as u64);
    kani::cover!(out.processed == 3 && out.attempts[0] == 1 && out.attempts[1] == 1 && out.attempts[2] == 1
        && matches!(pin.envs[0].out[0], NetOutcome::OsError) && matches!(pin.envs[1].out[0], NetOutcome::OtherError),
        "3 commands, 3 single attempts, despite timeout + error");
}
// @end

// @harness prop_d_fresh_commands_dropped stubs=poll_step
/// (d, limits) FINDING: two ways a FRESH command gets no reply at all.
///  1. backlog not yet flushed (boot-time flush failed): the poll slot is spent
///     on the flush, which confirms and discards whatever is queued, including
///     commands sent after the "online" message (contradicts main.py:566-567).
///  2. a successful /update reboots mid-batch; later commands in the batch are
///     never looked at, and the boot flush then discards them.
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(poll_step)]
fn prop_d_fresh_commands_dropped() {
    let (st, pin): (BotState, PollIn) = (kani::any(), kani::any());
    kani::assume(bot_ok(&st, 8) && poll_in_ok(&pin));
    let (_st1, out) = poll_step(st, pin);
    if !pin.flushed {
        assert!(out.processed == 0 && out.attempts[0] == 0 && out.attempts[1] == 0 && out.attempts[2] == 0);
    }
    if out.rebooted && out.processed == 1 {
        assert!(out.attempts[1] == 0 && out.attempts[2] == 0);
    }
    kani::cover!(!pin.flushed && out.flushed && pin.n >= 1
        && spec_is_fresh(&pin.updates[0], &pin.envs[0]) && matches!(pin.updates[0].cmd, Cmd::Status),
        "fresh /status discarded by the late flush, no reply");
    kani::cover!(out.rebooted && out.processed == 1 && pin.n == 3
        && spec_is_fresh(&pin.updates[1], &pin.envs[1]) && matches!(pin.updates[1].cmd, Cmd::Close),
        "fresh /close after /update in the same batch, no reply");
}
// @end

// @harness prop_wdt_poll_never_starves stubs=poll_step
/// Watchdog (WDT 300 s, fed at the top of every loop pass, main.py:608-609, and
/// before every HTTP call). Starting from a fresh feed, for any batch and any
/// outcome mix, the longest unfed stretch inside the poll block, and the stretch
/// still open after the 0.5 s loop sleep, stay below WATCHDOG_TIMEOUT_MS.
/// It does not grow with batch size: worst case is ONE HTTP call + one relay
/// press. ASSUMES each HTTP call returns within HTTP_CALL_MAX_MS (299 s; main.py
/// asks for 10 s). Door-monitor sends after the poll also feed first (by reading).
#[cfg(kani)]
#[kani::proof]
#[kani::stub_verified(poll_step)]
fn prop_wdt_poll_never_starves() {
    let (st, pin): (BotState, PollIn) = (kani::any(), kani::any());
    kani::assume(bot_ok(&st, 8) && poll_in_ok(&pin));
    kani::assume(st.wdt.gap == 0 && st.wdt.max_gap == 0); // just fed at loop top
    let (st1, _out) = poll_step(st, pin);
    assert!(st1.wdt.max_gap < WATCHDOG_TIMEOUT_MS);
    assert!(st1.wdt.gap + 500 < WATCHDOG_TIMEOUT_MS);
}
// @end

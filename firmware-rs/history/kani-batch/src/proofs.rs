//! Kani proof harnesses for the GarageBot port.
//!
//! Written by the PROOF agent from SPEC.md and INTERFACE.md only, without
//! reading the port's implementation. Expected values are recomputed here from
//! MicroPython's documented formulas, never by calling the port twice.
//!
//! Harnesses named `gap_*` state properties the spec wants but that reading
//! main.py suggested do NOT hold (SPEC.md "Known gaps"). A failure there is a
//! finding about the Python logic, not noise.

use crate::close_timeout::{CloseWatch, CLOSE_TIMEOUT_MS};
use crate::poll::{
    handle_command_replies, Action, FlushResponse, Net, PollEnv, PollState, Replies, Request,
    Update, UpdateKind,
};
use crate::stale::{stale_command_age, Cmd, MAX_COMMAND_AGE_SECONDS};
use crate::ticks::{
    door_open_minutes, poll_due, ticks_diff, ticks_ms_at, MAX_INTERVAL_MS, POLL_INTERVAL_MS,
    TICKS_HALF, TICKS_MASK, TICKS_PERIOD,
};
use crate::uptime::{Uptime, HEARTBEAT_INTERVAL_MS};

// ---------- independent spec helpers ----------

/// MicroPython v1.26.1 ESP32: period 2^30 ms.
const SPEC_PERIOD: i64 = 1 << 30;
const SPEC_HALF: i64 = 1 << 29;
const THIRTY_DAYS_MS: u64 = 30 * 24 * 3600 * 1000;
const HUNDRED_YEARS_S: u64 = 100 * 31_557_600;

/// extmod/modtime.c: ((end - start + PERIOD/2) & (PERIOD-1)) - PERIOD/2
fn spec_ticks_diff(end: u32, start: u32) -> i64 {
    (((end as i64) - (start as i64) + SPEC_HALF) & (SPEC_PERIOD - 1)) - SPEC_HALF
}

/// A real (ghost) start time whose tick value can be anywhere in the period.
fn any_real_start() -> u64 {
    let t: u64 = kani::any();
    kani::assume(t <= 1u64 << 40);
    t
}

fn any_ticks() -> u32 {
    let t: u32 = kani::any();
    kani::assume(t < (1u32 << 30));
    t
}

// =====================================================================
// (a) elapsed-time math across wraparound
// =====================================================================

#[kani::proof]
fn a_constants_match_micropython_esp32() {
    assert!(TICKS_PERIOD == 1 << 30);
    assert!(TICKS_MASK == (1 << 30) - 1);
    assert!(TICKS_HALF == 1 << 29);
    assert!(MAX_INTERVAL_MS == (1 << 29) - 1);
    assert!(POLL_INTERVAL_MS == 5_000);
    assert!(CLOSE_TIMEOUT_MS == 60_000);
    assert!(HEARTBEAT_INTERVAL_MS == 60_000);
    assert!(MAX_COMMAND_AGE_SECONDS == 60);
}

#[kani::proof]
fn a_ticks_ms_at_is_real_mod_period() {
    let real: u64 = kani::any();
    let t = ticks_ms_at(real);
    assert!(t < (1u32 << 30));
    assert!(t as u64 == real % (1u64 << 30));
}

/// The headline: true elapsed time is recovered for every start tick and every
/// elapsed time within the documented max interval, wrap included.
#[kani::proof]
fn a_ticks_diff_exact_within_max_interval() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d <= (1u64 << 29) - 1);
    let start = ticks_ms_at(t0);
    let end = ticks_ms_at(t0 + d);
    assert!(ticks_diff(end, start) as i64 == d as i64);
    // reversed arguments give the negated value
    assert!(ticks_diff(start, end) as i64 == -(d as i64));
    // the wrapped case is really exercised
    kani::cover!(end < start, "tick counter wrapped between start and end");
}

/// Matches MicroPython's formula for every pair of valid ticks; no overflow/panic.
#[kani::proof]
fn a_ticks_diff_matches_micropython_formula() {
    let a = any_ticks();
    let b = any_ticks();
    let d = ticks_diff(a, b) as i64;
    assert!(d == spec_ticks_diff(a, b));
    assert!(d >= -SPEC_HALF && d <= SPEC_HALF - 1);
}

// =====================================================================
// (b) close-timeout alert
// =====================================================================

/// One-step characterisation from ANY reachable-or-not state: the alert is sent
/// iff door open, bot close pending, not yet alerted, and ticks_diff >= 60 s.
#[kani::proof]
fn b_monitor_step_exact() {
    let has_time: bool = kani::any();
    let t = any_ticks();
    let before = CloseWatch {
        bot_triggered_close: kani::any(),
        close_requested_time: if has_time { Some(t) } else { None },
        close_timeout_alerted: kani::any(),
    };
    let door_open: bool = kani::any();
    let now = any_ticks();
    let mut w = before;
    let fired = w.monitor(door_open, now);

    let expect = door_open
        && before.bot_triggered_close
        && has_time
        && !before.close_timeout_alerted
        && spec_ticks_diff(now, t) >= 60_000;
    assert!(fired == expect);
    if !door_open {
        // seeing the door closed clears the request
        assert!(w == CloseWatch::default());
    } else if fired {
        assert!(w.close_timeout_alerted);
        assert!(w.bot_triggered_close == before.bot_triggered_close);
        assert!(w.close_requested_time == before.close_requested_time);
    } else {
        assert!(w == before);
    }
}

#[kani::proof]
fn b_close_command_arms_only_when_open() {
    let has_time: bool = kani::any();
    let before = CloseWatch {
        bot_triggered_close: kani::any(),
        close_requested_time: if has_time { Some(any_ticks()) } else { None },
        close_timeout_alerted: kani::any(),
    };
    let door_open: bool = kani::any();
    let now = any_ticks();
    let mut w = before;
    let pressed = w.on_close_command(door_open, now);
    assert!(pressed == door_open);
    if door_open {
        assert!(w.bot_triggered_close);
        assert!(w.close_requested_time == Some(now));
        assert!(!w.close_timeout_alerted); // re-armed
    } else {
        assert!(w == before); // "already closed": nothing changes
    }
}

/// Ghost-clock version of (b): door stays open after /close at real time t0.
/// Across a run of loop passes the warning fires iff the door has stayed open
/// for >= CLOSE_TIMEOUT, on the first such pass, exactly once. Wrap included.
#[kani::proof]
#[kani::unwind(5)]
fn b_fires_iff_open_longer_than_timeout_ghost_clock() {
    let t0 = any_real_start();
    kani::assume(t0 >= 300_000);
    let mut w = CloseWatch::default();
    assert!(w.on_close_command(true, ticks_ms_at(t0)));

    // Same loop iteration: monitor sees the current_time sampled BEFORE the handler ran.
    let early: u64 = kani::any();
    kani::assume(early <= 300_000);
    assert!(!w.monitor(true, ticks_ms_at(t0 - early)));

    let mut t = t0;
    let mut fired: u8 = 0;
    let mut i = 0;
    while i < 3 {
        let dt: u64 = kani::any();
        kani::assume(dt <= (1u64 << 29) - 1);
        t += dt;
        kani::assume(t - t0 <= (1u64 << 29) - 1);
        let f = w.monitor(true, ticks_ms_at(t));
        let open_long_enough = t - t0 >= 60_000;
        assert!(f == (open_long_enough && fired == 0));
        if f {
            fired += 1;
        }
        // iff, as a state fact: alerted <=> door has been open >= 60 s
        assert!((fired == 1) == open_long_enough);
        i += 1;
    }
    kani::cover!(fired == 1 && ticks_ms_at(t) < ticks_ms_at(t0), "fired across a wrap");
}

/// Door seen closed before the timeout: no warning then, and none afterwards
/// when it opens again (until a new /close).
#[kani::proof]
fn b_no_alert_once_door_seen_closed() {
    let t0 = any_real_start();
    let mut w = CloseWatch::default();
    assert!(w.on_close_command(true, ticks_ms_at(t0)));
    assert!(!w.monitor(false, any_ticks()));
    assert!(!w.monitor(true, any_ticks()));
    assert!(!w.monitor(true, any_ticks()));
}

// =====================================================================
// (c) uptime accumulator
// =====================================================================

#[kani::proof]
fn c_uptime_new() {
    let now = any_ticks();
    let u = Uptime::new(now);
    assert!(u.uptime_seconds == 0);
    assert!(u.last_heartbeat_time == now);
}

/// One step from any state: monotonic, exact increment, and no overflow from any
/// accumulated value up to u64::MAX - 600_000 s (far beyond 100 years).
#[kani::proof]
fn c_uptime_step_monotonic_no_overflow() {
    let before = Uptime {
        uptime_seconds: kani::any(),
        last_heartbeat_time: any_ticks(),
    };
    kani::assume(before.uptime_seconds <= u64::MAX - 600_000);
    assert!(HUNDRED_YEARS_S <= u64::MAX - 600_000);
    let now = any_ticks();
    let mut u = before;
    let fired = u.step(now);
    let d = spec_ticks_diff(now, before.last_heartbeat_time);

    assert!(u.uptime_seconds >= before.uptime_seconds); // monotonic
    assert!(fired == (d >= 60_000));
    if fired {
        assert!(u.uptime_seconds == before.uptime_seconds + (d / 1000) as u64);
        assert!(u.uptime_seconds >= before.uptime_seconds + 60);
        assert!(u.last_heartbeat_time == now);
    } else {
        assert!(u == before);
    }
}

/// Against the ghost clock, one inductive step from any accumulated uptime up to
/// 100 years: for a true pause `d` within the max interval, a heartbeat fires iff
/// d >= 60 s, and uptime then grows by floor(d / 1000): never ahead of real time,
/// behind by < 1 s per heartbeat. Wrap included. (By induction this covers every
/// run; a multi-step version with 64-bit multiplies did not finish in CBMC.)
#[kani::proof]
fn c_uptime_tracks_real_time_step() {
    let t0 = any_real_start();
    let d: u32 = kani::any();
    kani::assume(d <= (1u32 << 29) - 1);
    let start: u64 = kani::any();
    kani::assume(start <= HUNDRED_YEARS_S);
    let mut u = Uptime { uptime_seconds: start, last_heartbeat_time: ticks_ms_at(t0) };
    let fired = u.step(ticks_ms_at(t0 + d as u64));
    assert!(fired == (d >= 60_000));
    let gained = u.uptime_seconds - start; // also: no decrease (would underflow)
    if fired {
        assert!(gained == (d / 1000) as u64);
        assert!(d - (gained as u32) * 1000 <= 999);
    } else {
        assert!(gained == 0);
    }
    kani::cover!(fired && ticks_ms_at(t0 + d as u64) < ticks_ms_at(t0), "heartbeat across a wrap");
}

// =====================================================================
// stale-command guard
// =====================================================================

fn spec_guarded(c: Cmd) -> bool {
    matches!(c, Cmd::Open | Cmd::Close | Cmd::Update)
}

fn any_unix() -> i64 {
    let t: i64 = kani::any();
    kani::assume(t >= -(1i64 << 40) && t <= (1i64 << 40));
    t
}

fn spec_stale(clock_synced: bool, cmd: Cmd, date: Option<i64>, now: i64) -> Option<i64> {
    if !clock_synced || !spec_guarded(cmd) {
        return None;
    }
    match date {
        Some(d) if d != 0 && now - d > 60 => Some(now - d),
        _ => None,
    }
}

#[kani::proof]
fn s_stale_command_age_exact() {
    let cmd: Cmd = kani::any();
    assert!(cmd.is_stale_guarded() == spec_guarded(cmd));
    let synced: bool = kani::any();
    let has: bool = kani::any();
    let date = if has { Some(any_unix()) } else { None };
    let now = any_unix();
    assert!(stale_command_age(synced, cmd, date, now) == spec_stale(synced, cmd, date, now));
}

/// Robustness: no arithmetic overflow for arbitrary i64 inputs (Python ints
/// cannot overflow; a Rust port can).
#[kani::proof]
fn s_stale_command_age_never_panics() {
    let _ = stale_command_age(kani::any(), kani::any(), kani::any(), kani::any());
}

// =====================================================================
// (d) always responds
// =====================================================================

struct OneEnv {
    now: i64,
    upd: bool,
    now_calls: u8,
    upd_calls: u8,
    act_calls: u8,
    acted_before_now: bool,
    last_id: u64,
    last: Action,
}

impl PollEnv for OneEnv {
    fn now_unix(&mut self) -> i64 {
        self.now_calls += 1;
        if self.act_calls > 0 {
            self.acted_before_now = true;
        }
        self.now
    }
    fn update_available(&mut self) -> bool {
        self.upd_calls += 1;
        self.upd
    }
    fn act(&mut self, update: &Update, action: Action) {
        self.act_calls += 1;
        self.last_id = update.update_id;
        self.last = action;
    }
}

fn one_env() -> OneEnv {
    OneEnv {
        now: any_unix(),
        upd: kani::any(),
        now_calls: 0,
        upd_calls: 0,
        act_calls: 0,
        acted_before_now: false,
        last_id: 0,
        last: Action::Skip,
    }
}

fn any_kind() -> UpdateKind {
    let k: UpdateKind = kani::any();
    if let UpdateKind::Command { date: Some(d), .. } = k {
        kani::assume(d >= -(1i64 << 40) && d <= (1i64 << 40));
    }
    k
}

fn spec_replies(cmd: Cmd, update_available: bool) -> Replies {
    match cmd {
        Cmd::Unknown => Replies::Zero,
        Cmd::Update if update_available => Replies::Several,
        _ => Replies::One,
    }
}

#[kani::proof]
fn d_handle_command_replies_table() {
    let cmd: Cmd = kani::any();
    let upd: bool = kani::any();
    assert!(handle_command_replies(cmd, upd) == spec_replies(cmd, upd));
}

/// The decision for one delivered update, from any poll state:
/// exactly one `act`, offset advances, and the reply count is what main.py does.
#[kani::proof]
fn d_process_update_exactly_one_decision() {
    let mut st = PollState {
        last_update_id: kani::any(),
        backlog_flushed: true,
        clock_synced: kani::any(),
    };
    let synced = st.clock_synced;
    let u = Update { update_id: kani::any(), kind: any_kind() };
    let mut env = one_env();
    let a = st.process_update(&u, &mut env);

    assert!(env.act_calls == 1);
    assert!(env.last == a && env.last_id == u.update_id);
    assert!(env.now_calls <= 1 && !env.acted_before_now);
    assert!(st.last_update_id == u.update_id); // never handed out twice
    assert!(st.backlog_flushed && st.clock_synced == synced);

    match u.kind {
        UpdateKind::NonText | UpdateKind::Unauthorized => {
            assert!(a == Action::Skip);
            assert!(a.replies() == Replies::Zero);
            assert!(env.now_calls == 0 && env.upd_calls == 0);
        }
        UpdateKind::Command { cmd, date } => match spec_stale(synced, cmd, date, env.now) {
            Some(age) => {
                // refused: not executed, one "Ignored" notice (main.py behaviour)
                assert!(a == Action::IgnoredStale { age });
                assert!(a.replies() == Replies::One);
                assert!(env.upd_calls == 0);
            }
            None => {
                let r = spec_replies(cmd, env.upd);
                assert!(a == Action::Handled { cmd, replies: r });
                assert!(a.replies() == r);
                assert!(env.upd_calls == if matches!(cmd, Cmd::Update) { 1 } else { 0 });
            }
        },
    }
}

/// (d) as stated: a fresh, recognised command gets exactly one reply attempt
/// and is executed. (`update` with a new version is the designed exception.)
#[kani::proof]
fn d_fresh_command_exactly_one_reply() {
    let mut st = PollState {
        last_update_id: kani::any(),
        backlog_flushed: true,
        clock_synced: kani::any(),
    };
    let cmd: Cmd = kani::any();
    kani::assume(!matches!(cmd, Cmd::Unknown));
    let mut env = one_env();
    kani::assume(!(matches!(cmd, Cmd::Update) && env.upd));
    let sent = any_unix();
    kani::assume(env.now - sent <= 60); // fresh
    let u = Update { update_id: kani::any(), kind: UpdateKind::Command { cmd, date: Some(sent) } };
    let a = st.process_update(&u, &mut env);
    assert!(a == Action::Handled { cmd, replies: Replies::One });
    assert!(env.act_calls == 1);
}

/// With a synced clock, a guarded command older than 60 s is never executed.
#[kani::proof]
fn d_stale_guarded_never_executed_when_clock_synced() {
    let mut st = PollState { last_update_id: kani::any(), backlog_flushed: true, clock_synced: true };
    let cmd: Cmd = kani::any();
    kani::assume(spec_guarded(cmd));
    let mut env = one_env();
    let sent = any_unix();
    kani::assume(sent != 0 && env.now - sent > 60);
    let u = Update { update_id: kani::any(), kind: UpdateKind::Command { cmd, date: Some(sent) } };
    let a = st.process_update(&u, &mut env);
    assert!(!matches!(a, Action::Handled { .. }));
    assert!(env.upd_calls == 0);
}

const K: usize = 3;

struct SeqEnv {
    base: u64,
    acts: [u8; K],
    stray: bool,
    now: i64,
    upd: bool,
}

impl PollEnv for SeqEnv {
    fn now_unix(&mut self) -> i64 {
        self.now
    }
    fn update_available(&mut self) -> bool {
        self.upd
    }
    fn act(&mut self, update: &Update, _action: Action) {
        let i = update.update_id.wrapping_sub(self.base + 1);
        if i < K as u64 {
            self.acts[i as usize] += 1;
        } else {
            self.stray = true;
        }
    }
}

/// Bounded run against a ghost Telegram server: boot flush + 3 more rounds, each
/// request succeeding, timing out or erroring; up to 3 updates arriving at any
/// points (some already queued at boot = sent while offline).
///
/// - nothing is requested but Flush until a flush succeeds;
/// - a failed request changes nothing (so nothing is lost or repeated);
/// - updates queued when the flush succeeds are never acted on (no reply, no execution);
/// - every update delivered afterwards is acted on exactly once, never twice,
///   and the next offset neither skips an undelivered update nor re-requests an old one.
#[kani::proof]
#[kani::unwind(5)]
fn d_poll_sequence_once_each_and_backlog_dropped() {
    let base: u64 = kani::any();
    kani::assume(base <= 1u64 << 40);
    let updates = [
        Update { update_id: base + 1, kind: any_kind() },
        Update { update_id: base + 2, kind: any_kind() },
        Update { update_id: base + 3, kind: any_kind() },
    ];
    let mut env = SeqEnv { base, acts: [0; K], stray: false, now: any_unix(), upd: kani::any() };
    let mut st = PollState::boot(kani::any());
    assert!(st.last_update_id == 0 && !st.backlog_flushed);

    let mut arrived: usize = kani::any(); // queued while offline
    kani::assume(arrived <= K);
    let mut flushed = false;
    let mut cut: usize = 0; // how many were queued when the flush succeeded
    let mut next: usize = 0; // first update the bot has not consumed yet

    let mut round = 0;
    while round < 4 {
        let more: usize = kani::any();
        kani::assume(more <= K - arrived);
        arrived += more;

        let before = st;
        let outcome: u8 = kani::any();
        kani::assume(outcome < 3);
        match st.next_request() {
            Request::Flush => {
                assert!(!flushed);
                if outcome == 0 {
                    let ok: bool = kani::any();
                    let newest = if arrived > 0 { Some(base + arrived as u64) } else { None };
                    st.on_flush(Net::Ok(FlushResponse { ok, newest }));
                    if ok {
                        assert!(st.backlog_flushed);
                        flushed = true;
                        cut = arrived;
                        next = arrived;
                    } else {
                        assert!(st == before);
                    }
                } else {
                    st.on_flush(if outcome == 1 { Net::Timeout } else { Net::Error });
                    assert!(st == before);
                }
            }
            Request::GetUpdates { offset } => {
                assert!(flushed); // never poll for commands before the backlog is gone
                assert!(offset <= base + next as u64 + 1); // skips nothing undelivered
                assert!(next == 0 || offset == base + next as u64 + 1); // re-requests nothing
                if outcome == 0 {
                    st.on_updates(Net::Ok(&updates[next..arrived]), &mut env);
                    next = arrived;
                } else {
                    st.on_updates(if outcome == 1 { Net::Timeout } else { Net::Error }, &mut env);
                    assert!(st == before);
                }
            }
        }

        assert!(!env.stray);
        let mut i = 0;
        while i < K {
            let want = if flushed && i >= cut && i < next { 1 } else { 0 };
            assert!(env.acts[i] == want);
            i += 1;
        }
        round += 1;
    }
    kani::cover!(flushed && cut > 0 && next == K, "backlog dropped and fresh updates handled");
}

// =====================================================================
// (f) open-door minutes, within the max interval
// =====================================================================

#[kani::proof]
fn f_door_open_minutes_exact_within_max_interval() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d <= (1u64 << 29) - 1);
    let m = door_open_minutes(ticks_ms_at(t0 + d), ticks_ms_at(t0));
    assert!(m as i64 == (d / 60_000) as i64);
}

#[kani::proof]
fn g_poll_due_exact_within_max_interval() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d <= (1u64 << 29) - 1);
    assert!(poll_due(ticks_ms_at(t0 + d), ticks_ms_at(t0)) == (d >= 5_000));
}

// =====================================================================
// gap_*: properties the spec wants but main.py is suspected not to satisfy.
// Each failure here is a finding; see SPEC.md "Known gaps".
// =====================================================================

/// Gap 1: door open for up to 30 days; the minutes in the alert stay truthful.
#[kani::proof]
fn gap_door_open_minutes_truthful_up_to_30_days() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d <= THIRTY_DAYS_MS);
    let m = door_open_minutes(ticks_ms_at(t0 + d), ticks_ms_at(t0));
    assert!(m >= 0);
    assert!(m as i64 == (d / 60_000) as i64);
}

/// Gap 2a: after ANY pause of at least the poll interval (e.g. a long WiFi
/// outage spent inside connect_wifi), the next loop pass polls Telegram.
#[kani::proof]
fn gap_poll_due_after_any_pause() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d >= 5_000 && d <= THIRTY_DAYS_MS);
    assert!(poll_due(ticks_ms_at(t0 + d), ticks_ms_at(t0)));
}

/// Gap 2b: same for the heartbeat / uptime.
#[kani::proof]
fn gap_heartbeat_after_any_pause() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d >= 60_000 && d <= THIRTY_DAYS_MS);
    let mut u = Uptime::new(ticks_ms_at(t0));
    assert!(u.step(ticks_ms_at(t0 + d)));
    assert!(u.uptime_seconds == d / 1000);
}

/// Gap 2c: same for the close-timeout warning.
#[kani::proof]
fn gap_close_timeout_after_any_pause() {
    let t0 = any_real_start();
    let d: u64 = kani::any();
    kani::assume(d >= 60_000 && d <= THIRTY_DAYS_MS);
    let mut w = CloseWatch::default();
    assert!(w.on_close_command(true, ticks_ms_at(t0)));
    assert!(w.monitor(true, ticks_ms_at(t0 + d)));
}

struct CountEnv {
    now: i64,
    acts: u8,
}

impl PollEnv for CountEnv {
    fn now_unix(&mut self) -> i64 {
        self.now
    }
    fn update_available(&mut self) -> bool {
        false
    }
    fn act(&mut self, _update: &Update, _action: Action) {
        self.acts += 1;
    }
}

/// Gap 3: a command sent AFTER boot (queue was empty at the boot flush, so it is
/// not something queued while offline) is never dropped without being acted on,
/// whatever the network does.
#[kani::proof]
#[kani::unwind(4)]
fn gap_command_sent_after_boot_never_dropped() {
    let mut st = PollState::boot(kani::any());
    // boot-time flush; nothing queued
    let o: u8 = kani::any();
    kani::assume(o < 3);
    match st.next_request() {
        Request::Flush => {
            if o == 0 {
                st.on_flush(Net::Ok(FlushResponse { ok: kani::any(), newest: None }));
            } else {
                st.on_flush(if o == 1 { Net::Timeout } else { Net::Error });
            }
        }
        Request::GetUpdates { .. } => { assert!(false); }
    }
    // "bot is online" goes out; someone answers with a fresh command
    let id: u64 = kani::any();
    kani::assume(id >= 1 && id <= 1u64 << 40);
    let now = any_unix();
    let cmd: Cmd = kani::any();
    kani::assume(!matches!(cmd, Cmd::Unknown));
    let u = [Update { update_id: id, kind: UpdateKind::Command { cmd, date: Some(now) } }];
    let mut env = CountEnv { now, acts: 0 };

    let mut round = 0;
    while round < 2 {
        let o: u8 = kani::any();
        kani::assume(o < 3);
        match st.next_request() {
            Request::Flush => {
                if o == 0 {
                    st.on_flush(Net::Ok(FlushResponse { ok: kani::any(), newest: Some(id) }));
                } else {
                    st.on_flush(if o == 1 { Net::Timeout } else { Net::Error });
                }
            }
            Request::GetUpdates { offset } => {
                if o == 0 {
                    if offset <= id {
                        st.on_updates(Net::Ok(&u[..]), &mut env);
                    } else {
                        st.on_updates(Net::Ok(&u[..0]), &mut env);
                    }
                } else {
                    st.on_updates(if o == 1 { Net::Timeout } else { Net::Error }, &mut env);
                }
            }
        }
        // the bot has told Telegram to forget the command only if it acted on it
        if let Request::GetUpdates { offset } = st.next_request() {
            assert!(!(offset > id && env.acts == 0));
        }
        assert!(env.acts <= 1);
        round += 1;
    }
}

/// Gap 4: a guarded command (open/close/update) older than 60 s is never
/// executed, whether or not NTP ever succeeded.
#[kani::proof]
fn gap_stale_guarded_never_executed_any_clock_state() {
    let mut st = PollState {
        last_update_id: kani::any(),
        backlog_flushed: true,
        clock_synced: kani::any(),
    };
    let cmd: Cmd = kani::any();
    kani::assume(spec_guarded(cmd));
    let mut env = one_env();
    let sent = any_unix();
    kani::assume(sent != 0 && env.now - sent > 60);
    let u = Update { update_id: kani::any(), kind: UpdateKind::Command { cmd, date: Some(sent) } };
    let a = st.process_update(&u, &mut env);
    assert!(!matches!(a, Action::Handled { .. }));
}

/// The task's literal wording of (d): "stale commands are never replied to".
/// main.py deliberately sends one "Ignored ..." notice for an age-stale guarded
/// command, so this is expected to fail as a WRONG PROPERTY, kept for the record.
#[kani::proof]
fn literal_stale_command_gets_no_reply() {
    let mut st = PollState { last_update_id: kani::any(), backlog_flushed: true, clock_synced: true };
    let cmd: Cmd = kani::any();
    kani::assume(spec_guarded(cmd));
    let mut env = one_env();
    let sent = any_unix();
    kani::assume(sent != 0 && env.now - sent > 60);
    let u = Update { update_id: kani::any(), kind: UpdateKind::Command { cmd, date: Some(sent) } };
    let a = st.process_update(&u, &mut env);
    assert!(a.replies() == Replies::Zero);
}

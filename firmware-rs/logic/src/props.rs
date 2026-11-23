// Top-level properties, proven from CONTRACTS ALONE: every logic function a
// harness calls is replaced by its specification (kani::stub(f, spec_f)), so these proofs do
// not depend on any body and stay valid whatever bodies are written later, as
// long as each body's own contract_NAME proof passes.
//
// Each harness has kani::cover! checks: an unreachable cover fails the run, so
// a property cannot pass vacuously because its assumptions contradict each other.

/// The gap to the previous pass is within the watchdog limit (no overrun).
#[cfg(kani)]
fn gap_ok(last: Option<u64>, now: u64) -> bool {
    match last {
        Some(t) => now >= t && now - t <= WDT_TIMEOUT_MS,
        None => true,
    }
}

// ---------------------------------------------------------------- always responds
// @harness prop_always_responds target= stubs=on_update,step uses=
/// A known command from the authorized chat that the bot acts on gets exactly one
/// reply: /status and /debug always, a guarded command (/open, /close, /silence,
/// /update) only when its age is Fresh. A guarded command that is not fresh is
/// dropped silently instead (prop_not_fresh_is_silent).
/// on_update queues the reply, and with WiFi up it is handed to the shell within two
/// passes (only the boot greeting may go first), then the slot is empty again.
/// Holds while the bot is not deaf (prop_deaf_reboots); what happens when the
/// send fails is prop_send_retry.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
#[kani::stub(step, spec_step)]
fn prop_always_responds() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    let door: bool = kani::any();
    kani::assume(s.batch_left > 0 && s.pend_reply.is_none());
    kani::assume(u.authorized && u.cmd != Cmd::Unknown);
    kani::assume(u.update_id >= s.next_offset && u.update_id < MAX_UPDATE_ID);
    kani::assume(!spec_is_guarded(u.cmd) || matches!(spec_age(s.anchor, s.boot_unix, now, u.date), Age::Fresh(_)));
    let (s1, _press) = on_update(s, u, now, door);
    assert!(s1.pend_reply.is_some());
    let o = s1.pend_reply.unwrap();
    assert!(o.who == u.who);

    let i1: StepIn = kani::any();
    kani::assume(i1.wifi_up && gap_ok(s1.last_step, i1.now_ms));
    kani::assume(!spec_deaf(s1.last_good_poll, i1.now_ms));
    let r1 = step(s1, i1);
    if r1.op == Op::Send(Msg::Reply(o)) {
        assert!(r1.state.pend_reply.is_none());
        kani::cover!(true, "reply goes out in the first pass");
    } else {
        assert!(r1.op == Op::Send(Msg::Online));
        assert!(r1.state.pend_reply == Some(o));
        let i2: StepIn = kani::any();
        kani::assume(i2.wifi_up && gap_ok(r1.state.last_step, i2.now_ms));
        kani::assume(!spec_deaf(r1.state.last_good_poll, i2.now_ms));
        let r2 = step(r1.state, i2);
        assert!(r2.op == Op::Send(Msg::Reply(o)));
        assert!(r2.state.pend_reply.is_none());
        kani::cover!(true, "reply goes out in the second pass");
    }
}
// @end

// @harness prop_reply_exactly_once target= stubs=step uses=
/// step never invents a reply, never sends one twice, and never drops one unsent.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_reply_exactly_once() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let r = step(s, i);
    if let Op::Send(Msg::Reply(o)) = r.op {
        assert!(s.pend_reply == Some(o));
        assert!(r.state.pend_reply.is_none());
        kani::cover!(true, "a reply is sent");
    } else {
        assert!(r.state.pend_reply == s.pend_reply);
        kani::cover!(s.pend_reply.is_some(), "a reply waits");
    }
}
// @end

// @harness prop_next_update_waits_for_reply target= stubs=step,on_update uses=
/// A queued reply is never overwritten: step does not ask for the next update
/// while a reply waits, and on_update refuses to run if asked anyway.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(on_update, spec_on_update)]
fn prop_next_update_waits_for_reply() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let r = step(s, i);
    if r.op == Op::NextUpdate {
        assert!(r.state.pend_reply.is_none() && r.state.batch_left > 0);
        kani::cover!(true, "next update requested");
    }
    let s2: State = kani::any();
    kani::assume(s2.pend_reply.is_some());
    let (n, press) = on_update(s2, kani::any(), kani::any(), kani::any());
    assert!(n == s2 && !press);
}
// @end

// ---------------------------------------------------------------- stale commands never execute
// @harness prop_anchor_is_sound target= stubs=on_poll uses=
/// Ghost real time, written as (whole unix seconds, ms remainder < 1000) pairs so
/// that no assumption needs a 64-bit multiply or divide (a SAT solver cannot do
/// those; see CONTRACTS.md "Ghost time is split"). Pairs compare digit by digit,
/// which is the same order as the ms values they stand for.
///   req  = (req_s, req_r)  true unix time when the request was sent (monotonic t_req_ms)
///   up   = (up_s, up_r)    true time elapsed between monotonic 0 and the request
///   boot = req - up        true unix time at monotonic 0; its whole second is boot_s
/// Assumptions (TRUSTED.md: Telegram Date header, clock physics):
///   A1  the server wrote Date = D no earlier than the request left:  req < (D+1) s,  i.e. req_s <= D
///   A2  between boot and the request the monotonic clock gained at most 1 s on real time:
///       t_req_ms <= up + 1000 ms
/// Then every anchor on_poll stores is sound (the true time at its mono_ms is
/// below its unix_s), and a boot bound it stores is above the true boot time.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_poll, spec_on_poll)]
fn prop_anchor_is_sound() {
    let s: State = kani::any();
    let p: PollResult = kani::any();
    let now: u64 = kani::any();
    let req_s: u64 = kani::any();
    let req_r: u64 = kani::any();
    let up_s: u64 = kani::any();
    let up_r: u64 = kani::any();
    kani::assume(req_r < 1000 && up_r < 1000 && req_s <= (1u64 << 60) / 1000);
    // boot = req - up is a valid unix time (not negative)
    kani::assume(up_s < req_s || (up_s == req_s && up_r <= req_r));
    let boot_s = if req_r >= up_r { req_s - up_s } else { req_s - up_s - 1 };
    if let Some(d) = p.date_unix {
        kani::assume(d < MAX_UNIX_S);
        kani::assume(req_s <= d); // A1
    }
    let (tq, tr) = (p.t_req_ms / 1000, p.t_req_ms % 1000);
    kani::assume(tq < up_s + 1 || (tq == up_s + 1 && tr <= up_r)); // A2
    let n = on_poll(s, p, now);
    if n.anchor != s.anchor {
        let k = n.anchor.unwrap();
        assert!(k.mono_ms == p.t_req_ms);
        assert!(k.unix_s <= MAX_UNIX_S);
        assert!(req_s < k.unix_s); // true time at the anchor < unix_s * 1000 ms
        kani::cover!(true, "a new anchor is stored");
    }
    if n.boot_unix != s.boot_unix {
        assert!(s.boot_unix.is_none());
        let b = n.boot_unix.unwrap();
        assert!(boot_s < b); // true boot time < b * 1000 ms
        assert!(b <= MAX_UNIX_S + 1);
        kani::cover!(true, "the boot bound is fixed");
        kani::cover!(b == boot_s + 1, "the boot bound can be tight");
    }
    // a failed poll never touches the clock
    if !p.ok {
        assert!(n.anchor == s.anchor && n.boot_unix == s.boot_unix);
    }
}
// @end

// @harness prop_stale_never_executes target= stubs=on_update uses=
/// THE property: the relay is pressed only for a command that is truly at most
/// STALE_AFTER_S old and was sent after this boot.
/// Ghost real time, split as in prop_anchor_is_sound:
///   k    = (k_s, k_r)  true unix time at the anchor's mono_ms
///   el                 true ms elapsed from then until now  (true now = k + el)
///   boot_s             whole second of the true boot time
/// Assumptions:
///   the state's anchor and boot bound are sound (what prop_anchor_is_sound proves):
///       k_s < anchor.unix_s,  boot_s < boot_unix
///   A3  over any window up to ANCHOR_MAX_AGE_MS the monotonic clock loses at most 1 s on real time:
///       el <= (now - mono_ms) + 1000
///   A4  message.date is the true send time cut to whole seconds (send_ms >= date*1000)
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
fn prop_stale_never_executes() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    let door: bool = kani::any();
    let k_s: u64 = kani::any();
    let k_r: u64 = kani::any();
    let el: u64 = kani::any();
    let boot_s: u64 = kani::any();
    kani::assume(k_r < 1000 && k_s <= (1u64 << 60) / 1000 && el < (1u64 << 60));
    if let Some(k) = s.anchor {
        kani::assume(k.unix_s <= MAX_UNIX_S);
        kani::assume(k_s < k.unix_s); // anchor is sound
        if now >= k.mono_ms && now - k.mono_ms <= ANCHOR_MAX_AGE_MS {
            kani::assume(el <= (now - k.mono_ms) + 1000); // A3
        }
    }
    if let Some(b) = s.boot_unix {
        kani::assume(b <= MAX_UNIX_S + 1 && boot_s < b); // boot bound is sound
    }
    let (n, press) = on_update(s, u, now, door);
    if press {
        assert!(u.authorized);
        assert!(u.cmd == Cmd::Open || u.cmd == Cmd::Close);
        assert!(s.anchor.is_some() && s.boot_unix.is_some() && u.date.is_some());
        let d = u.date.unwrap();
        // a press only happens inside the anchor window, so el is small and the
        // carry from the ms remainder fits a 32-bit divide
        assert!(el <= ANCHOR_MAX_AGE_MS + 1000);
        let now_s = k_s + ((k_r + el) as u32 / 1000) as u64; // whole second of true now
        // send_ms >= d*1000 (A4), so true age <= true now - d*1000:
        // true now < (d + STALE_AFTER_S) * 1000 ms
        assert!(now_s < d + STALE_AFTER_S);
        // sent after this boot: d * 1000 ms > true boot time
        assert!(d > boot_s);
        // and the door was in the state that makes the press meaningful
        assert!((u.cmd == Cmd::Open) == !door);
        assert!(n.next_offset == u.update_id + 1);
        kani::cover!(u.cmd == Cmd::Open, "a fresh /open presses");
        kani::cover!(u.cmd == Cmd::Close, "a fresh /close presses");
        kani::cover!(now_s + 1 == d + STALE_AFTER_S, "a command at the very edge of the limit presses");
    }
    kani::cover!(!press && u.authorized && u.cmd == Cmd::Open && !door && s.anchor.is_some(), "an /open is refused");
}
// @end

// @harness prop_no_clock_no_press target= stubs=on_update uses=
/// Bug 3 of the MicroPython firmware: with no trustworthy clock, nothing is pressed,
/// whatever the command and whatever date it carries. A guarded command gets no reply
/// either: it is dropped silently (only its update id is consumed).
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
fn prop_no_clock_no_press() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    let stale_anchor = match s.anchor {
        Some(k) => now < k.mono_ms || now - k.mono_ms > ANCHOR_MAX_AGE_MS,
        None => true,
    };
    kani::assume(stale_anchor || s.boot_unix.is_none() || u.date.is_none());
    let (n, press) = on_update(s, u, now, kani::any());
    assert!(!press);
    assert!(n.door.close == s.door.close && n.update_pending == s.update_pending && n.door.muted == s.door.muted);
    let acted = s.batch_left > 0 && s.pend_reply.is_none() && u.authorized && u.update_id >= s.next_offset && u.update_id < MAX_UPDATE_ID;
    if acted && u.cmd == Cmd::Open {
        assert!(n.pend_reply.is_none());
        kani::cover!(true, "the /open is dropped silently");
    }
    kani::cover!(s.anchor.is_some() && u.date.is_some(), "anchor too old");
}
// @end

// @harness prop_not_fresh_is_silent target= stubs=on_update uses=
/// A guarded command (/open, /close, /silence, /update) from the authorized chat whose
/// age is not Fresh (no usable clock, sent before this boot, or older than
/// STALE_AFTER_S) is dropped silently: no press, no reply, nothing armed, muted or
/// queued. It is still consumed: next_offset moves past it, so it is never read again.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
fn prop_not_fresh_is_silent() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    kani::assume(s.batch_left > 0 && s.pend_reply.is_none());
    kani::assume(u.authorized && spec_is_guarded(u.cmd));
    kani::assume(u.update_id >= s.next_offset && u.update_id < MAX_UPDATE_ID);
    let age = spec_age(s.anchor, s.boot_unix, now, u.date);
    kani::assume(!matches!(age, Age::Fresh(_)));
    let (n, press) = on_update(s, u, now, kani::any());
    assert!(!press);
    assert!(n.next_offset == u.update_id + 1);
    assert!(n.batch_left == s.batch_left - 1);
    let mut m = n;
    m.next_offset = s.next_offset;
    m.batch_left = s.batch_left;
    assert!(m == s);
    kani::cover!(age == Age::Unknown, "no usable clock: dropped");
    kani::cover!(age == Age::PreBoot, "sent before this boot: dropped");
    kani::cover!(matches!(age, Age::Stale(_)), "too old: dropped");
}
// @end

// @harness prop_unauthorized_never_acts target= stubs=on_update uses=
/// A message from any other chat changes nothing but the read position.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
fn prop_unauthorized_never_acts() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    kani::assume(!u.authorized);
    let (n, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    let mut m = n;
    m.next_offset = s.next_offset;
    m.batch_left = s.batch_left;
    assert!(m == s);
    kani::cover!(n.next_offset != s.next_offset, "the update is still consumed");
}
// @end

// @harness prop_never_twice target= stubs=on_update,on_poll,step uses=
/// No update is processed twice: one below next_offset is inert, processing
/// moves next_offset past the id, nothing ever moves next_offset back, and a
/// poll always asks from next_offset.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
#[kani::stub(on_poll, spec_on_poll)]
#[kani::stub(step, spec_step)]
fn prop_never_twice() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let (n, press) = on_update(s, u, kani::any(), kani::any());
    assert!(n.next_offset >= s.next_offset);
    if u.update_id < s.next_offset {
        assert!(!press && n.pend_reply == s.pend_reply && n.door == s.door && n.update_pending == s.update_pending);
        kani::cover!(s.batch_left > 0 && s.pend_reply.is_none(), "a replayed update is skipped");
    }
    if press || n.pend_reply != s.pend_reply {
        assert!(n.next_offset == u.update_id + 1);
        kani::cover!(true, "an update is acted on");
    }
    let s2: State = kani::any();
    assert!(on_poll(s2, kani::any(), kani::any()).next_offset == s2.next_offset);
    let r = step(s2, kani::any());
    assert!(r.state.next_offset == s2.next_offset);
    if let Op::Poll { offset } = r.op {
        assert!(offset == s2.next_offset && s2.batch_left == 0);
        kani::cover!(true, "a poll is issued");
    }
}
// @end

// ---------------------------------------------------------------- polls and /update
// @harness prop_failed_poll_loses_nothing target= stubs=on_poll uses=
/// A failed poll changes nothing except "not quiesced"; a poll result that
/// arrives while a batch is still being worked on is ignored entirely.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_poll, spec_on_poll)]
fn prop_failed_poll_loses_nothing() {
    let s: State = kani::any();
    let p: PollResult = kani::any();
    let n = on_poll(s, p, kani::any());
    if s.batch_left > 0 {
        assert!(n == s);
    } else if !p.ok {
        let mut m = n;
        m.quiesced = s.quiesced;
        assert!(m == s && !n.quiesced);
        kani::cover!(true, "failed poll");
    } else {
        assert!(n.batch_left <= MAX_BATCH && n.quiesced == (p.count == 0));
        assert!(n.pend_reply == s.pend_reply && n.door == s.door && n.update_pending == s.update_pending);
        kani::cover!(n.batch_left == MAX_BATCH, "a full batch");
    }
}
// @end

// @harness prop_update_keeps_batch target= stubs=on_update,step uses=
/// Bug 6: /update no longer swallows the commands behind it. It only sets a flag;
/// the batch counter goes down by exactly one; and the OTA op is issued only when
/// nothing is left to process or send and Telegram has confirmed an empty queue.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
#[kani::stub(step, spec_step)]
fn prop_update_keeps_batch() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    kani::assume(s.batch_left > 0 && s.pend_reply.is_none());
    let (n, press) = on_update(s, u, kani::any(), kani::any());
    assert!(n.batch_left == s.batch_left - 1);
    if u.cmd == Cmd::Update {
        assert!(!press);
        kani::cover!(n.update_pending && !s.update_pending && n.batch_left > 0, "/update queued with more commands behind it");
    }
    let s2: State = kani::any();
    let r = step(s2, kani::any());
    if r.op == Op::Ota {
        assert!(s2.update_pending && s2.quiesced && s2.batch_left == 0);
        assert!(!s2.pend_online && s2.pend_reply.is_none());
        assert!(r.state.pend_close_timeout.is_none() && r.state.pend_open_alert.is_none() && r.state.pend_closed_notice.is_none());
        assert!(!r.state.update_pending);
        kani::cover!(true, "OTA starts");
    } else {
        assert!(r.state.update_pending == s2.update_pending);
    }
    assert!(r.state.batch_left == s2.batch_left);
}
// @end

// ---------------------------------------------------------------- door
// @harness prop_close_alert target= stubs=step uses=
/// After a bot-triggered close, if the door is still open CLOSE_TIMEOUT_MS later
/// the requester is warned: exactly once, in this very pass if the line is free,
/// otherwise queued. WiFi being down does not stop the detection.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_close_alert() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    kani::assume(i.door_open);
    let c = match s.door.close {
        Some(c) => c,
        None => return,
    };
    let r = step(s, i);
    let timed_out = i.now_ms >= c.at && i.now_ms - c.at >= CLOSE_TIMEOUT_MS;
    let sent = r.op == Op::Send(Msg::CloseTimeout { who: c.who });
    if !c.alerted && timed_out {
        assert!(r.state.door.close == Some(CloseReq { at: c.at, who: c.who, alerted: true }));
        assert!(sent != (r.state.pend_close_timeout == Some(c.who)));
        if i.wifi_up && gap_ok(s.last_step, i.now_ms) && !spec_deaf(s.last_good_poll, i.now_ms) && !s.pend_online && s.pend_reply.is_none() {
            assert!(sent);
            kani::cover!(true, "warning sent in the same pass");
        }
        kani::cover!(!i.wifi_up && !sent, "warning queued while WiFi is down");
    } else {
        // no new warning: whatever goes out was already queued
        assert!(r.state.door.close == s.door.close);
        if let Op::Send(Msg::CloseTimeout { who }) = r.op {
            assert!(s.pend_close_timeout == Some(who));
        } else {
            assert!(r.state.pend_close_timeout == s.pend_close_timeout);
        }
        kani::cover!(c.alerted && timed_out, "already warned: not again");
    }
}
// @end

// @harness prop_closed_door_is_quiet target= stubs=step uses=
/// Once the door reads closed, no "door is open" alert and no "did not close"
/// warning is sent or left queued, and the door state is back to idle.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_closed_door_is_quiet() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    kani::assume(!i.door_open);
    let r = step(s, i);
    assert!(!matches!(r.op, Op::Send(Msg::OpenAlert { .. }) | Op::Send(Msg::CloseTimeout { .. })));
    assert!(r.state.pend_open_alert.is_none() && r.state.pend_close_timeout.is_none());
    assert!(r.state.door == DOOR_IDLE);
    kani::cover!(s.pend_open_alert.is_some(), "a queued alert is dropped");
    kani::cover!(matches!(r.op, Op::Send(Msg::ClosedAfter(_))), "the closed notice goes out");
}
// @end

// @harness prop_open_alert_schedule target= stubs=step uses=
/// Open-door alerts: first after INITIAL_ALERT_MS, then every REPEAT_ALERT_MS,
/// never while muted; and the minutes in the alert are spec_whole_minutes of
/// exactly (now - open_since), for every u64 duration.
///
/// spec_whole_minutes is replaced here by an opaque function (minutes_opaque):
/// at the one duration this pass is about it returns a value the harness chose
/// freely, anywhere else an arbitrary value. So the proof holds whatever the
/// function computes, and needs no 64-bit division (two dividers by 60_000 on
/// the same input are beyond the SAT solver; see CONTRACTS.md). What the
/// function computes is its one-line definition in spec.rs, `elapsed_ms / 1000 / 60`,
/// and prop_open_alert_minutes checks concrete durations against it.
#[cfg(kani)]
static mut MINUTES_AT: u64 = 0;
#[cfg(kani)]
static mut MINUTES_IS: u64 = 0;
#[cfg(kani)]
fn minutes_opaque(elapsed_ms: u64) -> u64 {
    if elapsed_ms == unsafe { MINUTES_AT } {
        unsafe { MINUTES_IS }
    } else {
        kani::any()
    }
}

#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(spec_whole_minutes, minutes_opaque)]
fn prop_open_alert_schedule() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    kani::assume(i.door_open);
    let s0 = match s.door.open_since {
        Some(t) => t,
        None => return,
    };
    kani::assume(i.now_ms >= s0);
    if let Some(a) = s.door.last_alert {
        kani::assume(i.now_ms >= a);
    }
    let e = i.now_ms - s0;
    let m: u64 = kani::any(); // stands for spec_whole_minutes(e)
    unsafe {
        MINUTES_AT = e;
        MINUTES_IS = m;
    }
    let due = match s.door.last_alert {
        None => e >= INITIAL_ALERT_MS,
        Some(a) => i.now_ms - a >= REPEAT_ALERT_MS,
    };
    let r = step(s, i);
    let sent = r.op == Op::Send(Msg::OpenAlert { minutes: m });
    if due && !s.door.muted {
        assert!(r.state.door.last_alert == Some(i.now_ms));
        assert!(sent != (r.state.pend_open_alert == Some(m)));
        kani::cover!(sent && s.door.last_alert.is_none(), "first alert");
        kani::cover!(sent && s.door.last_alert.is_some(), "repeat alert");
        kani::cover!(!sent, "alert waits behind a more urgent message");
    } else {
        assert!(r.state.door.last_alert == s.door.last_alert);
        if let Op::Send(Msg::OpenAlert { minutes }) = r.op {
            assert!(s.pend_open_alert == Some(minutes));
        } else {
            assert!(r.state.pend_open_alert == s.pend_open_alert);
        }
        kani::cover!(due && s.door.muted, "muted: no alert");
    }
    assert!(r.state.door.muted == s.door.muted && r.state.door.open_since == s.door.open_since);
}
// @end

// @harness prop_open_alert_minutes target= stubs=step uses=
/// The same alert with the real spec_whole_minutes, at concrete durations
/// (bug 4: at 7.57 days the old firmware reported a negative number).
/// The door opened at any monotonic time; only the duration is fixed.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_open_alert_minutes() {
    let mut s: State = kani::any();
    let mut i: StepIn = kani::any();
    let s0: u64 = kani::any();
    let which: u8 = kani::any();
    // (duration ms, whole minutes)
    let (e, m): (u64, u64) = match which {
        0 => (INITIAL_ALERT_MS, 15),
        1 => (INITIAL_ALERT_MS + 59_999, 15),
        2 => (INITIAL_ALERT_MS + 60_000, 16),
        3 => (654_048_000, 10_900),                      // 7.57 days
        4 => (u32::MAX as u64, 71_582),                  // 49.7 days: a 32-bit ms counter wraps
        5 => (u32::MAX as u64 + 1, 71_582),
        _ => (1u64 << 62, 76_861_433_640_456),           // far beyond any uptime
    };
    kani::assume(s0 <= u64::MAX - e);
    s.door.open_since = Some(s0);
    s.door.last_alert = None;
    s.door.muted = false;
    i.door_open = true;
    i.now_ms = s0 + e;
    let r = step(s, i);
    let sent = r.op == Op::Send(Msg::OpenAlert { minutes: m });
    assert!(sent != (r.state.pend_open_alert == Some(m)));
    kani::cover!(sent && which == 3, "7.57 days open: 10900 minutes, not negative");
    kani::cover!(sent && which == 6, "2^62 ms open");
}
// @end

// ---------------------------------------------------------------- watchdog, WiFi, time
// @harness prop_watchdog target= stubs=step,op_budget_ms uses=
/// The shell feeds the hardware watchdog once, right before each step. So:
///  (1) every op's budget plus loop slack is below the watchdog timeout: a shell
///      that honours budgets always feeds in time;
///  (2) if a pass nevertheless comes later than the timeout, step orders a reboot
///      itself (a second fence, should the hardware watchdog be misconfigured);
///  (3) step records the time of every pass.
/// "Never treated as late" in (2) is for a bot that is not deaf (prop_deaf_reboots).
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(op_budget_ms, spec_op_budget_ms)]
fn prop_watchdog() {
    let op: Op = kani::any();
    let b = op_budget_ms(op);
    assert!(b <= MAX_OP_BUDGET_MS);
    assert!(b + LOOP_SLACK_MS < WDT_TIMEOUT_MS);
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let r = step(s, i);
    assert!(r.state.last_step == Some(i.now_ms));
    if let Some(t) = s.last_step {
        if i.now_ms > t && i.now_ms - t > WDT_TIMEOUT_MS {
            assert!(r.op == Op::Reboot);
            kani::cover!(true, "late pass: reboot");
        }
        // a pass that follows within budget + slack is never treated as late
        let prev: Op = kani::any();
        if i.now_ms >= t && i.now_ms - t <= op_budget_ms(prev) + LOOP_SLACK_MS && i.wifi_up && !spec_deaf(s.last_good_poll, i.now_ms) {
            assert!(r.op != Op::Reboot);
            kani::cover!(true, "on-time pass: no reboot");
        }
    }
}
// @end

// @harness prop_wifi_never_blocks target= stubs=step,op_budget_ms uses=
/// Bug 1: WiFi being down cannot hang the device. The only ops are a bounded
/// connect attempt or, after WIFI_REBOOT_AFTER_MS of continuous outage, a reboot.
/// No network op is ever issued while WiFi is down.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(op_budget_ms, spec_op_budget_ms)]
fn prop_wifi_never_blocks() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let r = step(s, i);
    if !i.wifi_up {
        assert!(r.op == Op::WifiConnect || r.op == Op::Reboot);
        assert!(op_budget_ms(r.op) <= 20_000);
        let since = r.state.wifi_down_since.unwrap();
        assert!(since == s.wifi_down_since.unwrap_or(i.now_ms));
        if i.now_ms >= since && i.now_ms - since >= WIFI_REBOOT_AFTER_MS {
            assert!(r.op == Op::Reboot);
            kani::cover!(true, "long outage: reboot");
        }
        kani::cover!(r.op == Op::WifiConnect, "bounded reconnect");
    } else {
        assert!(r.state.wifi_down_since.is_none());
        assert!(r.op != Op::WifiConnect);
    }
}
// @end

// @harness prop_uptime_no_drift target= stubs=step uses=
/// Bug 5: the reported uptime is the monotonic clock itself, not a sum of
/// rounded intervals, so it is exact whatever the history.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_uptime_no_drift() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let r = step(s, i);
    match r.heartbeat {
        Some(up) => {
            assert!(up == i.now_ms / 1000);
            assert!(r.state.last_heartbeat == Some(i.now_ms));
            kani::cover!(up == 654_048, "7.57 days of uptime reported exactly");
        }
        None => {
            let t = s.last_heartbeat.unwrap();
            assert!(i.now_ms < t || i.now_ms - t < HEARTBEAT_INTERVAL_MS);
            assert!(r.state.last_heartbeat == s.last_heartbeat);
        }
    }
}
// @end

// @harness prop_boot target= stubs=step uses=
/// First pass after boot (within DEAF_REBOOT_AFTER_MS of power-on): greet if WiFi is up, else try to connect. Nothing is
/// polled, pressed or flushed before that (bug 2: there is no boot flush at all).
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_boot() {
    let i: StepIn = kani::any();
    kani::assume(i.now_ms < DEAF_REBOOT_AFTER_MS); // the first pass comes within 30 minutes of power-on
    let r = step(BOOT_STATE, i);
    if i.wifi_up {
        assert!(r.op == Op::Send(Msg::Online) && !r.state.pend_online);
    } else {
        assert!(r.op == Op::WifiConnect && r.state.pend_online);
    }
    assert!(r.heartbeat.is_some() && r.state.anchor.is_none());
    kani::cover!(i.wifi_up, "boot with WiFi");
    kani::cover!(!i.wifi_up, "boot without WiFi");
}
// @end

// @harness prop_clock_only_from_poll target= stubs=on_update,step uses=
/// The link between prop_anchor_is_sound and prop_stale_never_executes: the
/// anchor and the boot bound start empty and only on_poll ever writes them
/// (on_update and step pass them through untouched). A stored anchor is a fixed
/// pair of numbers, so once sound it stays sound. Hence every anchor and boot
/// bound on_update ever sees was stored by on_poll and is sound.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(on_update, spec_on_update)]
#[kani::stub(step, spec_step)]
fn prop_clock_only_from_poll() {
    assert!(BOOT_STATE.anchor.is_none() && BOOT_STATE.boot_unix.is_none());
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    let door: bool = kani::any();
    let (n, press) = on_update(s, u, now, door);
    assert!(n.anchor == s.anchor && n.boot_unix == s.boot_unix);
    let i: StepIn = kani::any();
    let r = step(s, i);
    assert!(r.state.anchor == s.anchor && r.state.boot_unix == s.boot_unix);
    kani::cover!(press && s.anchor.is_some(), "a pressing update leaves the clock alone");
    kani::cover!(r.op != Op::Idle && s.boot_unix.is_some(), "a busy pass leaves the clock alone");
}
// @end

// ---------------------------------------------------------------- fences added after the verifier's review
// @harness prop_clock_back_reboots target= stubs=step uses=
/// The monotonic clock is not trusted blindly: a pass whose time is before the
/// previous pass's time orders a reboot, whatever else is going on.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_clock_back_reboots() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let back = match s.last_step {
        Some(t) => i.now_ms < t,
        None => false,
    };
    let r = step(s, i);
    if back {
        assert!(r.op == Op::Reboot);
        kani::cover!(true, "clock went back");
    }
    kani::cover!(!back && r.op != Op::Reboot, "normal pass");
}
// @end

// @harness prop_deaf_reboots target= stubs=step,on_poll uses=
/// The bot cannot stay alive and deaf: DEAF_REBOOT_AFTER_MS without a successful
/// poll (counted from boot if there was none) orders a reboot, WiFi up or not.
/// Only a successful poll that is taken into account moves that time, to `now`.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(on_poll, spec_on_poll)]
fn prop_deaf_reboots() {
    assert!(BOOT_STATE.last_good_poll.is_none());
    let s: State = kani::any();
    let i: StepIn = kani::any();
    let since = match s.last_good_poll {
        Some(t) => t,
        None => 0,
    };
    let r = step(s, i);
    if i.now_ms >= since && i.now_ms - since >= DEAF_REBOOT_AFTER_MS {
        assert!(r.op == Op::Reboot);
        kani::cover!(i.wifi_up, "deaf with WiFi up");
    }
    assert!(r.state.last_good_poll == s.last_good_poll);

    let s2: State = kani::any();
    let p: PollResult = kani::any();
    let now: u64 = kani::any();
    let n = on_poll(s2, p, now);
    if p.ok && s2.batch_left == 0 {
        assert!(n.last_good_poll == Some(now));
        kani::cover!(true, "a good poll");
    } else {
        assert!(n.last_good_poll == s2.last_good_poll);
        kani::cover!(!p.ok, "a failed poll");
    }
}
// @end

// @harness prop_send_retry target= stubs=step,send_done uses=
/// A failed send is not lost. The shell reports every send with send_done:
///  (1) when step hands a message out, the slot it came from is empty;
///  (2) a failed send puts the message back into that slot, where it is queued
///      like any other message (all the properties above apply to it again);
///  (3) the exception: at SEND_TRIES failed sends in a row the message that
///      just failed is dropped and the count starts again, so one message that
///      cannot be sent never blocks the queue;
///  (4) send_done changes nothing but that one slot and the count, and step
///      never changes the count.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
#[kani::stub(send_done, spec_send_done)]
fn prop_send_retry() {
    assert!(BOOT_STATE.send_fails == 0);
    let s: State = kani::any();
    let i: StepIn = kani::any();
    kani::assume(s.send_fails < SEND_TRIES);
    let r = step(s, i);
    assert!(r.state.send_fails == s.send_fails);
    if let Op::Send(m) = r.op {
        let t = r.state;
        let ok: bool = kani::any();
        let n = send_done(t, m, ok);
        assert!(n.send_fails < SEND_TRIES);
        let keep = !ok && t.send_fails + 1 < SEND_TRIES;
        if ok {
            assert!(n.send_fails == 0);
        } else if keep {
            assert!(n.send_fails == t.send_fails + 1);
            kani::cover!(true, "kept for a retry");
        } else {
            assert!(n.send_fails == 0);
            kani::cover!(true, "dropped after SEND_TRIES failures in a row");
        }
        // the same state as after step, except the count and (if kept) m's slot
        let mut want = t;
        want.send_fails = n.send_fails;
        match m {
            Msg::Online => {
                assert!(!t.pend_online);
                want.pend_online = keep;
            }
            Msg::Reply(o) => {
                assert!(t.pend_reply.is_none());
                want.pend_reply = if keep { Some(o) } else { None };
                kani::cover!(keep, "a reply is kept");
            }
            Msg::CloseTimeout { who } => {
                assert!(t.pend_close_timeout.is_none());
                want.pend_close_timeout = if keep { Some(who) } else { None };
            }
            Msg::OpenAlert { minutes } => {
                assert!(t.pend_open_alert.is_none());
                want.pend_open_alert = if keep { Some(minutes) } else { None };
            }
            Msg::ClosedAfter(c) => {
                assert!(t.pend_closed_notice.is_none());
                want.pend_closed_notice = if keep { Some(c) } else { None };
            }
        }
        assert!(n == want);
    }
}
// @end

// Lemmas: the real spec functions have the facts their stand-ins assume
// (spec.rs, mod opaque).
// @harness lemma_div10_facts target= stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn lemma_div10_facts() {
    let x: u64 = kani::any();
    let (q, r) = spec_div10(x);
    assert!(opaque::div10_facts(x, q, r));
}
// @end

// @harness lemma_dec_index target= stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(22)]
#[kani::stub(spec_div10, opaque::div10_opaque)]
fn lemma_dec_index() {
    let v: u64 = kani::any();
    opaque::div10_chain(v);
    let (_, i) = spec_dec(v);
    assert!(i < 20);
    kani::cover!(i == 0, "twenty digits");
    kani::cover!(i == 19, "one digit");
}
// @end

// @harness lemma_json_escape_len target= stubs= uses=
/// The stand-in for `spec_json_escape` may assume a length of at most 6.
#[cfg(kani)]
#[kani::proof]
fn lemma_json_escape_len() {
    let c: u8 = kani::any();
    let (_, n) = spec_json_escape(c);
    assert!(n == 1 || n == 2 || n == 6);
}
// @end

// @harness lemma_parse_u64_18 target= stubs= uses=
/// The stand-in for `spec_parse_u64` may assume: at most 18 bytes give a value below 10^18.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(23)]
fn lemma_parse_u64_18() {
    let buf: [u8; 21] = kani::any();
    let n: usize = kani::any();
    kani::assume(n <= 18);
    if let Some(v) = spec_parse_u64(&buf[..n]) {
        assert!(v < 1_000_000_000_000_000_000);
    }
}
// @end

// @harness lemma_text_eq target= stubs= uses=
/// The stand-ins compare Texts 16 bytes at a time; that is the same as `==`.
#[cfg(kani)]
#[kani::proof]
fn lemma_text_eq() {
    let a: Text = kani::any();
    let b: Text = kani::any();
    assert!(opaque::gb_text_eq(&a, &b) == (a == b));
}
// @end

// ---------------------------------------------------------------- over-the-air update
// @harness prop_ota_only_newer target= stubs=version_newer,parse_version,ota_plan,ota_accept uses=
/// OTA (a): an image is downloaded or booted only when its version is strictly newer
/// than the running one. Never the same version again, never an older one, never the
/// version that just rolled back. "Newer" is a strict order, so it cannot hold both
/// ways. The two version strings this release uses parse as intended: the flashed
/// 2.0.0+ota is 2.0.0 (build metadata ignored) and v2.0.1 is newer than it.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(28)]
#[kani::stub(version_newer, spec_version_newer)]
#[kani::stub(parse_version, spec_parse_version)]
#[kani::stub(ota_plan, spec_ota_plan)]
#[kani::stub(ota_accept, spec_ota_accept)]
fn prop_ota_only_newer() {
    let running: Version = kani::any();
    let m: Manifest = kani::any();
    let slot: u32 = kani::any();
    let bad: Option<Version> = kani::any();
    let d: Download = kani::any();
    if ota_plan(running, m, slot, bad) {
        assert!(version_newer(m.version, running));
        assert!(m.version != running && !version_newer(running, m.version));
        assert!(bad != Some(m.version));
        kani::cover!(true, "a newer image is fetched");
    }
    if ota_accept(running, m, slot, d) {
        assert!(m.version != running && !version_newer(running, m.version));
        kani::cover!(true, "a newer image is booted");
    }
    // The same version is never fetched or booted again.
    let same = Manifest { version: running, ..m };
    assert!(!ota_plan(running, same, slot, bad) && !ota_accept(running, same, slot, d));
    // Two different versions: exactly one is newer.
    let a: Version = kani::any();
    if a != running {
        assert!(version_newer(a, running) != version_newer(running, a));
    }
    let flashed = parse_version(b"2.0.0+ota");
    let next = parse_version(b"v2.0.1");
    assert!(flashed == Some(Version { major: 2, minor: 0, patch: 0 }));
    assert!(next == Some(Version { major: 2, minor: 0, patch: 1 }));
    assert!(parse_version(b"2.0.1-rc1").is_none() && parse_version(b"2.0").is_none());
}
// @end

// @harness prop_ota_checks_image target= stubs=ota_accept uses=
/// OTA (b): the downloaded image becomes the boot image only when its size and SHA-256
/// are exactly the manifest's, the size is non-zero and fits the slot, esp_ota_end
/// accepted it (format, image hash, signature), and the version inside the image is
/// the manifest's. One wrong byte of the hash or one byte of size is enough to refuse.
#[cfg(kani)]
#[kani::proof]
#[kani::stub(ota_accept, spec_ota_accept)]
fn prop_ota_checks_image() {
    let running: Version = kani::any();
    let m: Manifest = kani::any();
    let slot: u32 = kani::any();
    let d: Download = kani::any();
    if ota_accept(running, m, slot, d) {
        assert!(d.size == m.size && d.sha256 == m.sha256);
        assert!(d.size > 0 && d.size <= slot);
        assert!(d.image_ok && d.image_version == Some(m.version));
        kani::cover!(true, "an image is accepted");
    }
    let k: usize = kani::any();
    kani::assume(k < 32);
    let mut wrong = d;
    wrong.sha256[k] ^= 1;
    if d.sha256 == m.sha256 {
        assert!(!ota_accept(running, m, slot, wrong));
    }
    let short = Download { size: m.size.wrapping_sub(1), ..d };
    assert!(!ota_accept(running, m, slot, short));
}
// @end

// @harness prop_unverified_needs_good_poll target= stubs=ota_mark_valid,on_poll,on_update,send_done uses=
/// OTA (c): a new image that has not completed a successful Telegram poll never marks
/// itself valid, so a reboot before that point makes the bootloader roll back. At boot
/// there has been no good poll; commands and sends never record one; a poll records
/// one only when it succeeded (HTTP 200, a complete body, "ok":true).
#[cfg(kani)]
#[kani::proof]
#[kani::stub(ota_mark_valid, spec_ota_mark_valid)]
#[kani::stub(on_poll, spec_on_poll)]
#[kani::stub(on_update, spec_on_update)]
#[kani::stub(send_done, spec_send_done)]
fn prop_unverified_needs_good_poll() {
    assert!(BOT_BOOT.core.last_good_poll.is_none());
    let pending: bool = kani::any();
    assert!(!ota_mark_valid(pending, None));
    assert!(!ota_mark_valid(false, kani::any()));
    let s: State = kani::any();
    kani::assume(s.last_good_poll.is_none());
    let (n, _) = on_update(s, kani::any(), kani::any(), kani::any());
    assert!(n.last_good_poll.is_none());
    let n = send_done(s, kani::any(), kani::any());
    assert!(n.last_good_poll.is_none());
    let p: PollResult = kani::any();
    let n = on_poll(s, p, kani::any());
    if ota_mark_valid(true, n.last_good_poll) {
        assert!(p.ok);
        kani::cover!(true, "a good poll confirms the image");
    } else {
        kani::cover!(!p.ok, "a failed poll leaves it unconfirmed");
    }
}
// @end

// @harness prop_step_keeps_unverified target= stubs=step uses=
/// OTA (c), the loop pass: a step never records a good poll by itself (only the
/// poll's own result does, above).
#[cfg(kani)]
#[kani::proof]
#[kani::stub(step, spec_step)]
fn prop_step_keeps_unverified() {
    let s: State = kani::any();
    kani::assume(s.last_good_poll.is_none());
    let r = step(s, kani::any());
    assert!(r.state.last_good_poll.is_none());
    kani::cover!(r.op == Op::Ota, "even an OTA pass");
}
// @end

// @harness prop_ota_check_cannot_starve target= stubs=bot_step,step uses=
/// OTA (d): the update check never gets in the way of commands. The pass that runs it
/// is chosen only when no update is waiting to be processed, no reply or notice is
/// waiting to be sent, and the last poll succeeded with nothing new; it clears the
/// request, so it runs once. The timer raises a request at most once per
/// OTA_CHECK_INTERVAL_MS, and the timer itself never touches the command queue.
/// (The pass has a bounded budget: prop_watchdog covers Op::Ota like every op.)
#[cfg(kani)]
#[kani::proof]
#[kani::stub(bot_step, spec_bot_step)]
#[kani::stub(step, spec_step)]
fn prop_ota_check_cannot_starve() {
    let b: Bot = kani::any();
    let i: StepIn = kani::any();
    let r = bot_step(b, i);
    if r.op == Op::Ota {
        assert!(b.core.batch_left == 0 && b.core.quiesced && i.wifi_up);
        assert!(b.core.pend_reply.is_none() && !b.core.pend_online);
        assert!(!r.bot.core.update_pending);
        kani::cover!(!b.core.update_pending, "the timer's own check runs");
    }
    assert!(r.bot.core.batch_left == b.core.batch_left && r.bot.core.next_offset == b.core.next_offset);
    if let Some(t) = b.ota_last_check {
        if i.now_ms >= t && i.now_ms - t < OTA_CHECK_INTERVAL_MS && !b.core.update_pending {
            assert!(r.op != Op::Ota && !r.bot.core.update_pending && r.bot.ota_last_check == b.ota_last_check);
            kani::cover!(true, "between checks");
        }
    }
    kani::cover!(b.ota_last_check.is_none() && !b.core.update_pending && r.bot.core.update_pending, "the boot check is requested");
}
// @end

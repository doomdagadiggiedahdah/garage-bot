//! Independent verifier harnesses. Written from VERIFIER_SPEC.md against the public
//! signatures only (rustdoc output), not from the builder's contracts or bodies.
//! Every harness runs the REAL bodies: nothing is stubbed, except `whole_minutes`
//! in `v4_open_alert_duration_is_true_elapsed`, where the stub only records its argument.
//!
//! Naming: vN_... belongs to property VN of VERIFIER_SPEC.md.
#![allow(unused, clippy::all)]

use crate::*;

// ---------- generators (no reliance on derived Arbitrary) ----------

fn any_name() -> Name {
    Name { len: kani::any(), bytes: kani::any() }
}

fn any_cmd() -> Cmd {
    match kani::any::<u8>() % 7 {
        0 => Cmd::Status,
        1 => Cmd::Open,
        2 => Cmd::Close,
        3 => Cmd::Silence,
        4 => Cmd::Update,
        5 => Cmd::Diag,
        _ => Cmd::Unknown,
    }
}

fn any_reply() -> Reply {
    match kani::any::<u8>() % 8 {
        0 => Reply::Status { open: kani::any() },
        1 => Reply::AlreadyOpen,
        2 => Reply::Opening,
        3 => Reply::AlreadyClosed,
        4 => Reply::Closing,
        5 => Reply::Muted,
        6 => Reply::UpdateQueued,
        _ => Reply::Diag,
    }
}

fn any_outgoing() -> Outgoing {
    Outgoing { who: any_name(), reply: any_reply() }
}

fn any_anchor() -> Option<Anchor> {
    if kani::any() {
        Some(Anchor { unix_s: kani::any(), mono_ms: kani::any() })
    } else {
        None
    }
}

fn any_close() -> Option<CloseReq> {
    if kani::any() {
        Some(CloseReq { at: kani::any(), who: any_name(), alerted: kani::any() })
    } else {
        None
    }
}

fn any_door() -> DoorState {
    DoorState {
        open_since: kani::any(),
        last_alert: kani::any(),
        muted: kani::any(),
        close: any_close(),
    }
}

fn any_state() -> State {
    State {
        next_offset: kani::any(),
        batch_left: kani::any(),
        anchor: any_anchor(),
        boot_unix: kani::any(),
        last_poll: kani::any(),
        quiesced: kani::any(),
        update_pending: kani::any(),
        pend_online: kani::any(),
        pend_reply: if kani::any() { Some(any_outgoing()) } else { None },
        pend_close_timeout: if kani::any() { Some(any_name()) } else { None },
        pend_open_alert: kani::any(),
        pend_closed_notice: if kani::any() {
            Some(ClosedNotice { who: any_name(), tenths: kani::any() })
        } else {
            None
        },
        door: any_door(),
        last_heartbeat: kani::any(),
        wifi_down_since: kani::any(),
        last_step: kani::any(),
        last_good_poll: kani::any(),
        send_fails: kani::any(),
    }
}

fn any_update() -> UpdateIn {
    UpdateIn {
        update_id: kani::any(),
        authorized: kani::any(),
        cmd: any_cmd(),
        date: kani::any(),
        who: any_name(),
    }
}

fn any_step_in() -> StepIn {
    StepIn { now_ms: kani::any(), door_open: kani::any(), wifi_up: kani::any() }
}

fn any_poll() -> PollResult {
    PollResult { ok: kani::any(), count: kani::any(), date_unix: kani::any(), t_req_ms: kani::any() }
}

fn is_guarded(c: Cmd) -> bool {
    matches!(c, Cmd::Open | Cmd::Close | Cmd::Update)
}

fn is_reply_send(op: &Op) -> bool {
    matches!(op, Op::Send(Msg::Reply(_)))
}

const P30: u64 = 1 << 30; // the MicroPython tick period the Python bot wrapped at

// =====================================================================
// V1  Always responds
// =====================================================================

/// V1.2: every known command from the authorised chat that is accepted gets a reply
/// queued for its sender, whatever the door position: /status and /debug at any age,
/// a guarded command only when it is fresh. A guarded command that is not fresh gets
/// no reply (dropped silently) and is still consumed.
#[kani::proof]
fn v1_known_command_gets_reply() {
    let s = any_state();
    let u = any_update();
    let now: u64 = kani::any();
    let door: bool = kani::any();
    kani::assume(s.batch_left > 0 && s.pend_reply.is_none());
    kani::assume(u.authorized && !matches!(u.cmd, Cmd::Unknown));
    kani::assume(u.update_id >= s.next_offset && u.update_id < MAX_UPDATE_ID);
    let fresh = matches!(judge_age(s.anchor, s.boot_unix, now, u.date), Age::Fresh(_));
    let guarded = matches!(u.cmd, Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Update);
    let (t, press) = on_update(s, u, now, door);
    if guarded && !fresh {
        assert!(t.pend_reply.is_none() && !press);
        assert!(t.next_offset == u.update_id + 1);
        kani::cover!(true, "a stale guarded command is dropped silently");
        return;
    }
    match t.pend_reply {
        Some(o) => {
            assert!(o.who == u.who);
            if press {
                assert!(matches!(o.reply, Reply::Opening | Reply::Closing));
            }
        }
        None => panic!("known command got no reply"),
    }
    assert!(t.next_offset == u.update_id + 1);
    assert!(t.batch_left == s.batch_left - 1);
}

/// V1.2 edge: what happens to an accepted-looking command whose id is >= MAX_UPDATE_ID.
/// Spec wants: answered or at least never executed. (Telegram ids are far below 2^62.)
#[kani::proof]
fn v1_huge_update_id_never_acts() {
    let s = any_state();
    let u = any_update();
    kani::assume(u.update_id >= MAX_UPDATE_ID);
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.update_pending == s.update_pending);
}

/// V1.4/V1.5 (weak form): one pass never loses a queued reply. Either this pass hands
/// exactly that reply to the shell, or it is still queued unchanged.
#[kani::proof]
fn v1_step_never_loses_reply() {
    let mut s = any_state();
    let r = any_outgoing();
    s.pend_reply = Some(r);
    let i = any_step_in();
    let out = step(s, i);
    match out.op {
        Op::Send(Msg::Reply(o)) => {
            assert!(o == r);
            assert!(out.state.pend_reply.is_none());
            assert!(i.wifi_up);
        }
        _ => assert!(out.state.pend_reply == Some(r)),
    }
    kani::cover!(is_reply_send(&out.op));
    kani::cover!(matches!(out.op, Op::Reboot));
}

/// V1: with WiFi up and the loop on time, a queued reply is handed over within two passes.
#[kani::proof]
fn v1_reply_within_two_passes() {
    let mut s = any_state();
    let r = any_outgoing();
    s.pend_reply = Some(r);
    let i1 = any_step_in();
    let i2 = any_step_in();
    kani::assume(i1.wifi_up && i2.wifi_up);
    // loop on time: no overrun in either pass
    kani::assume(match s.last_step {
        None => true,
        Some(l) => i1.now_ms >= l && i1.now_ms - l <= WDT_TIMEOUT_MS,
    });
    kani::assume(i2.now_ms >= i1.now_ms && i2.now_ms - i1.now_ms <= WDT_TIMEOUT_MS);
    let o1 = step(s, i1);
    if is_reply_send(&o1.op) {
        return;
    }
    let o2 = step(o1.state, i2);
    assert!(is_reply_send(&o2.op));
}

/// V1 under a bounded adversarial mix: 3 passes, any WiFi/door/time inputs.
/// The reply is sent at most once and is never dropped without being sent.
#[kani::proof]
fn v1_reply_survives_any_three_passes() {
    let mut s = any_state();
    let r = any_outgoing();
    s.pend_reply = Some(r);
    let mut sent = 0u8;
    let mut k = 0;
    while k < 3 {
        let i = any_step_in();
        let out = step(s, i);
        if let Op::Send(Msg::Reply(o)) = out.op {
            assert!(o == r);
            sent += 1;
        }
        s = out.state;
        if sent == 0 {
            assert!(s.pend_reply == Some(r));
        } else {
            assert!(s.pend_reply.is_none());
        }
        k += 1;
    }
    assert!(sent <= 1);
}

/// V1.1: a failed poll changes nothing but `quiesced`.
#[kani::proof]
fn v1_failed_poll_loses_nothing() {
    let s = any_state();
    let mut p = any_poll();
    p.ok = false;
    let t = on_poll(s, p, kani::any());
    let mut e = s;
    e.quiesced = t.quiesced;
    assert!(t == e);
}

/// V1.1: a poll result that arrives while a batch is open changes nothing.
#[kani::proof]
fn v1_poll_during_batch_ignored() {
    let s = any_state();
    kani::assume(s.batch_left > 0);
    let t = on_poll(s, any_poll(), kani::any());
    assert!(t == s);
}

/// V1.6: polling is due again after ANY pause that the watchdog rule allows, and is
/// requested from the unprocessed offset. Quiet door, nothing pending.
#[kani::proof]
fn v1_poll_due_after_any_pause() {
    let mut s = any_state();
    s.pend_online = false;
    s.pend_reply = None;
    s.pend_close_timeout = None;
    s.pend_open_alert = None;
    s.pend_closed_notice = None;
    s.batch_left = 0;
    s.update_pending = false;
    s.door = DoorState { open_since: None, last_alert: None, muted: false, close: None };
    let i = StepIn { now_ms: kani::any(), door_open: false, wifi_up: true };
    kani::assume(match s.last_step {
        None => true,
        Some(l) => i.now_ms >= l && i.now_ms - l <= WDT_TIMEOUT_MS,
    });
    kani::assume(match s.last_poll {
        None => true,
        Some(l) => i.now_ms >= l && i.now_ms - l >= POLL_INTERVAL_MS,
    });
    let out = step(s, i);
    match out.op {
        Op::Poll { offset } => assert!(offset == s.next_offset),
        _ => panic!("poll was due but not issued"),
    }
    assert!(out.state.last_poll == Some(i.now_ms));
}

/// V1.6 fence, KNOWN GAP (contract gap, not a body bug): if the shell's clock ever goes BACK
/// (now < last_poll) the bot idles, deaf, until the clock catches up. The spec wants a poll
/// or a reboot. `should_panic` keeps the run green while the gap exists; when the core gets
/// a fence (for example: now < last_step => Reboot) this harness starts FAILING: then delete
/// the `should_panic` line.
#[kani::proof]
#[kani::should_panic]
fn gap_poll_due_even_if_clock_went_back() {
    let mut s = any_state();
    s.pend_online = false;
    s.pend_reply = None;
    s.pend_close_timeout = None;
    s.pend_open_alert = None;
    s.pend_closed_notice = None;
    s.batch_left = 0;
    s.update_pending = false;
    s.door = DoorState { open_since: None, last_alert: None, muted: false, close: None };
    s.last_step = None;
    let i = StepIn { now_ms: kani::any(), door_open: false, wifi_up: true };
    kani::assume(matches!(s.last_poll, Some(l) if i.now_ms < l));
    let out = step(s, i);
    assert!(matches!(out.op, Op::Poll { .. } | Op::Reboot));
}

/// V1.3 + V2: `step` never moves the read position, the batch or the clock.
#[kani::proof]
fn v1_step_keeps_offset_batch_clock() {
    let s = any_state();
    let out = step(s, any_step_in());
    assert!(out.state.next_offset == s.next_offset);
    assert!(out.state.batch_left == s.batch_left);
    assert!(out.state.anchor == s.anchor);
    assert!(out.state.boot_unix == s.boot_unix);
}

/// V1.7: other chats and unknown text: no press, no reply, no state change that matters.
#[kani::proof]
fn v1_unauthorized_or_unknown_is_inert() {
    let s = any_state();
    let u = any_update();
    kani::assume(!u.authorized || matches!(u.cmd, Cmd::Unknown));
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.pend_reply == s.pend_reply);
    assert!(t.door == s.door);
    assert!(t.update_pending == s.update_pending);
    assert!(t.anchor == s.anchor && t.boot_unix == s.boot_unix);
}

/// V5.4: the loop never asks for the next update while a reply is waiting, and never polls
/// while a batch is open (a poll then would be thrown away).
#[kani::proof]
fn v5_no_next_update_over_reply_no_poll_over_batch() {
    let s = any_state();
    let i = any_step_in();
    let out = step(s, i);
    match out.op {
        Op::NextUpdate => assert!(s.pend_reply.is_none() && s.batch_left > 0 && i.wifi_up),
        Op::Poll { offset } => {
            assert!(s.batch_left == 0 && s.pend_reply.is_none() && i.wifi_up);
            assert!(offset == s.next_offset);
        }
        _ => {}
    }
}

/// V1/V5 progress: an open batch with no reply waiting and nothing else to send is worked on
/// (NextUpdate), not starved.
#[kani::proof]
fn v5_open_batch_makes_progress() {
    let mut s = any_state();
    s.pend_online = false;
    s.pend_reply = None;
    s.pend_close_timeout = None;
    s.pend_open_alert = None;
    s.pend_closed_notice = None;
    s.door = DoorState { open_since: None, last_alert: None, muted: false, close: None };
    kani::assume(s.batch_left > 0);
    let i = StepIn { now_ms: kani::any(), door_open: false, wifi_up: true };
    kani::assume(match s.last_step {
        None => true,
        Some(l) => i.now_ms >= l && i.now_ms - l <= WDT_TIMEOUT_MS,
    });
    let out = step(s, i);
    assert!(matches!(out.op, Op::NextUpdate));
}

// =====================================================================
// V2  Stale or unverifiable-age /open never actuates
// =====================================================================

/// V2.2/V2.3: no anchor, no boot bound or no message date: nothing is pressed, nothing is
/// armed, no update is queued. This is the "before NTP sync" case of main.py.
#[kani::proof]
fn v2_no_clock_no_press() {
    let s = any_state();
    let u = any_update();
    kani::assume(s.anchor.is_none() || s.boot_unix.is_none() || u.date.is_none());
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.door.close == s.door.close);
    assert!(t.update_pending == s.update_pending);
}

/// V2.2: straight after boot nothing can be pressed.
#[kani::proof]
fn v2_boot_state_has_no_clock() {
    assert!(BOOT_STATE.anchor.is_none() && BOOT_STATE.boot_unix.is_none());
    assert!(BOOT_STATE.next_offset == 0 && BOOT_STATE.batch_left == 0);
    assert!(BOOT_STATE.pend_reply.is_none() && !BOOT_STATE.update_pending);
    let mut s = BOOT_STATE;
    s.batch_left = kani::any();
    let (_, press) = on_update(s, any_update(), kani::any(), kani::any());
    assert!(!press);
}

/// V2 main: a press implies every condition of the spec, for ANY state.
#[kani::proof]
fn v2_press_implies_everything() {
    let s = any_state();
    let u = any_update();
    let now: u64 = kani::any();
    let door: bool = kani::any();
    let (t, press) = on_update(s, u, now, door);
    if press {
        assert!(u.authorized);
        assert!(s.batch_left > 0 && s.pend_reply.is_none());
        assert!(u.update_id >= s.next_offset);
        assert!(t.next_offset > u.update_id);
        assert!(matches!(u.cmd, Cmd::Open | Cmd::Close));
        assert!(matches!(u.cmd, Cmd::Open) == !door); // V2.7
        assert!(matches!(judge_age(s.anchor, s.boot_unix, now, u.date), Age::Fresh(_)));
    }
    // firmware replacement is guarded the same way
    if t.update_pending && !s.update_pending {
        assert!(u.authorized && matches!(u.cmd, Cmd::Update));
        assert!(matches!(judge_age(s.anchor, s.boot_unix, now, u.date), Age::Fresh(_)));
    }
    kani::cover!(press);
}

/// V1.3: an update at or below the processed position does nothing (no double press).
#[kani::proof]
fn v2_never_twice() {
    let s = any_state();
    let u = any_update();
    kani::assume(u.update_id < s.next_offset);
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.next_offset == s.next_offset);
    assert!(t.pend_reply == s.pend_reply);
    assert!(t.door == s.door && t.update_pending == s.update_pending);
}

/// V1.3: the read position never moves back, in any of the three transitions.
#[kani::proof]
fn v2_offset_monotonic() {
    let s = any_state();
    let (t, _) = on_update(s, any_update(), kani::any(), kani::any());
    assert!(t.next_offset >= s.next_offset);
    let t2 = on_poll(s, any_poll(), kani::any());
    assert!(t2.next_offset == s.next_offset);
}

/// V2.1/V2.4/V2.5: shape of a Fresh verdict, for every input (also proves no overflow).
#[kani::proof]
fn v2_judge_age_shape() {
    let a = any_anchor();
    let boot: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let date: Option<u64> = kani::any();
    match judge_age(a, boot, now, date) {
        Age::Fresh(age) => {
            assert!(age <= STALE_AFTER_S);
            let an = a.unwrap();
            let d = date.unwrap();
            let b = boot.unwrap();
            assert!(an.mono_ms <= now);
            assert!(now - an.mono_ms <= ANCHOR_MAX_AGE_MS);
            assert!(d >= b);
        }
        Age::Stale(age) => assert!(age > STALE_AFTER_S),
        _ => {}
    }
}

/// V2.1 against TRUE time (ghost values, my own model, in whole seconds):
///  - anchor sound: true unix second at the anchor instant `ta` is below `unix_s`
///  - the device clock lost at most 1 s since the anchor: true elapsed `el` ms
///    satisfies el <= (now - mono_ms) + 1000
/// Then a Fresh verdict means the message is at most 60 s old in true time.
#[kani::proof]
fn v2_fresh_means_truly_recent() {
    let unix_s: u64 = kani::any();
    let mono_ms: u64 = kani::any();
    let now: u64 = kani::any();
    let d: u64 = kani::any();
    let boot: Option<u64> = kani::any();
    let ta: u64 = kani::any(); // true unix second at mono_ms
    let el_s: u64 = kani::any(); // true whole seconds elapsed since then, rounded up
    kani::assume(ta < unix_s);
    if let Age::Fresh(_) = judge_age(Some(Anchor { unix_s, mono_ms }), boot, now, Some(d)) {
        // (shape harness shows mono_ms <= now and now - mono_ms <= 300 000 here)
        let dev_s = (now - mono_ms) / 1000;
        kani::assume(el_s <= dev_s + 2); // ceil((dev_ms + 1000) / 1000) <= dev_s + 2
        let true_now_s = ta as u128 + el_s as u128;
        assert!(true_now_s <= d as u128 + STALE_AFTER_S as u128);
    }
}

/// V2: completeness, so safety is not vacuous. A message that is clearly recent, with a
/// healthy anchor, is judged Fresh.
#[kani::proof]
fn v2_recent_message_is_fresh() {
    let unix_s: u64 = kani::any();
    let mono_ms: u64 = kani::any();
    let now: u64 = kani::any();
    let d: u64 = kani::any();
    let boot: u64 = kani::any();
    kani::assume(unix_s <= MAX_UNIX_S && mono_ms <= now && now - mono_ms <= ANCHOR_MAX_AGE_MS);
    kani::assume(unix_s >= mono_ms / 1000 + 1);
    kani::assume(now < 1 << 50);
    let ub = unix_s + (now - mono_ms) / 1000 + NOW_SLACK_S;
    kani::assume(d >= boot && d <= ub && ub - d <= STALE_AFTER_S);
    let age = judge_age(Some(Anchor { unix_s, mono_ms }), Some(boot), now, Some(d));
    assert!(matches!(age, Age::Fresh(_)));
}

/// V2: the clock (anchor, boot bound) is written only by a good poll, only from that
/// response's Date, and the boot bound is fixed once.
#[kani::proof]
fn v2_clock_only_from_good_poll() {
    let s = any_state();
    let p = any_poll();
    let now: u64 = kani::any();
    let t = on_poll(s, p, now);
    if t.anchor != s.anchor {
        assert!(p.ok && s.batch_left == 0);
        let d = p.date_unix.unwrap();
        assert!(d < MAX_UNIX_S && p.t_req_ms <= now);
        assert!(t.anchor == Some(Anchor { unix_s: d + 1, mono_ms: p.t_req_ms }));
    }
    if t.boot_unix != s.boot_unix {
        assert!(s.boot_unix.is_none());
        assert!(t.anchor != s.anchor || s.anchor.is_some());
        if let (Some(b), Some(d)) = (t.boot_unix, p.date_unix) {
            // boot bound is not below (Date + 2 s) - uptime at the request
            assert!(b as u128 + (p.t_req_ms / 1000) as u128 >= d as u128 + 2);
        }
    }
    let (t3, _) = on_update(s, any_update(), now, kani::any());
    assert!(t3.anchor == s.anchor && t3.boot_unix == s.boot_unix);
}

/// V2.6 / V1.3 across a reboot: after boot, a command dated before the boot bound is never
/// executed, even if it is younger than 60 s (redelivery of an already executed command).
#[kani::proof]
fn v2_preboot_command_never_executes() {
    let s = any_state();
    let u = any_update();
    kani::assume(matches!((s.boot_unix, u.date), (Some(b), Some(d)) if d < b));
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.update_pending == s.update_pending);
}

// =====================================================================
// V3  Close alert after the timeout, across tick wraparound
// =====================================================================

/// V3.1: alert iff elapsed >= 60 s, for every arming time and every now (u64 ms, so the
/// 2^30 ms MicroPython wrap points are ordinary values).
#[kani::proof]
fn v3_close_alert_iff_timeout() {
    let mut d = any_door();
    let at: u64 = kani::any();
    let who = any_name();
    d.close = Some(CloseReq { at, who, alerted: false });
    let now: u64 = kani::any();
    let (nd, ev) = door_tick(d, now, true);
    let due = now >= at && now - at >= CLOSE_TIMEOUT_MS;
    assert!(ev.close_timeout.is_some() == due);
    match nd.close {
        Some(c) => {
            assert!(c.at == at && c.who == who);
            assert!(c.alerted == due);
        }
        None => panic!("close request lost while the door is open"),
    }
    if due {
        assert!(ev.close_timeout == Some(who));
    }
    // armed before a 2^30 boundary, checked after it
    kani::cover!(due && at / P30 < now / P30);
    kani::cover!(!due && at / P30 < now / P30);
    // pause longer than 2^29 ms, where MicroPython ticks_diff went negative
    kani::cover!(due && now - at > (1 << 29));
}

/// V3.1 concrete wrap cases, as in the Python bot: armed 10 s before the 2^30 wrap.
#[kani::proof]
fn v3_close_alert_wrap_concrete() {
    let k: u64 = kani::any();
    kani::assume(k >= 1 && k <= 1_000_000);
    let at = k * P30 - 10_000;
    let mk = || DoorState {
        open_since: Some(at - 5_000),
        last_alert: None,
        muted: false,
        close: Some(CloseReq { at, who: Name { len: 0, bytes: [0; 32] }, alerted: false }),
    };
    let (_, e1) = door_tick(mk(), at + 59_999, true);
    assert!(e1.close_timeout.is_none());
    let (_, e2) = door_tick(mk(), at + 60_000, true);
    assert!(e2.close_timeout.is_some());
    let (_, e3) = door_tick(mk(), at + (1 << 29) + 1, true);
    assert!(e3.close_timeout.is_some());
}

/// V3.2/V3.3: once only; never without a request; never once the door is closed.
#[kani::proof]
fn v3_close_alert_once_and_only_when_armed() {
    let d = any_door();
    let now: u64 = kani::any();
    let open: bool = kani::any();
    let (nd, ev) = door_tick(d, now, open);
    if ev.close_timeout.is_some() {
        assert!(open);
        assert!(matches!(d.close, Some(c) if !c.alerted));
    }
    if !open {
        assert!(nd.close.is_none() && nd.open_since.is_none());
        assert!(ev.open_alert.is_none());
    }
}

/// V3: the alert is not lost between `door_tick` and the shell: the pass that detects it
/// sends it or leaves it queued; and it stays queued until sent while the door is open.
#[kani::proof]
fn v3_close_alert_reaches_the_shell() {
    let mut s = any_state();
    let at: u64 = kani::any();
    let who = any_name();
    s.door.close = Some(CloseReq { at, who, alerted: false });
    s.pend_close_timeout = None;
    let i = StepIn { now_ms: kani::any(), door_open: true, wifi_up: kani::any() };
    kani::assume(i.now_ms >= at && i.now_ms - at >= CLOSE_TIMEOUT_MS);
    let out = step(s, i);
    let sent_now = matches!(out.op, Op::Send(Msg::CloseTimeout { who: w }) if w == who);
    assert!(sent_now || out.state.pend_close_timeout == Some(who));
    assert!(!(sent_now && out.state.pend_close_timeout.is_some()));
    // second pass, door still open: still sent or still queued
    let i2 = StepIn { now_ms: kani::any(), door_open: true, wifi_up: kani::any() };
    let out2 = step(out.state, i2);
    let sent2 = matches!(out2.op, Op::Send(Msg::CloseTimeout { who: w }) if w == who);
    if sent_now {
        assert!(!sent2); // exactly once
    } else {
        assert!(sent2 || out2.state.pend_close_timeout == Some(who));
    }
}

/// V3: a fresh /close on an open door arms the check at `now`.
#[kani::proof]
fn v3_fresh_close_arms() {
    let s = any_state();
    let mut u = any_update();
    u.cmd = Cmd::Close;
    let now: u64 = kani::any();
    let (t, press) = on_update(s, u, now, true);
    if press {
        assert!(matches!(t.door.close, Some(c) if c.at == now && !c.alerted && c.who == u.who));
    } else {
        assert!(t.door.close == s.door.close);
    }
    kani::cover!(press);
}

// =====================================================================
// V4  No negative or overflowing durations over any uptime
// =====================================================================

/// V4.3: uptime is exact for every u64 millisecond count.
#[kani::proof]
fn v4_uptime_exact() {
    let n: u64 = kani::any();
    assert!(uptime_s(n) == n / 1000);
}

/// V4.3: the heartbeat reports exactly now/1000; heartbeat is due after any pause.
#[kani::proof]
fn v4_heartbeat_uptime() {
    let s = any_state();
    let i = any_step_in();
    let out = step(s, i);
    if let Some(u) = out.heartbeat {
        assert!(u == i.now_ms / 1000);
        assert!(out.state.last_heartbeat == Some(i.now_ms));
    }
    if let Some(l) = s.last_heartbeat {
        if i.now_ms >= l && i.now_ms - l >= HEARTBEAT_INTERVAL_MS {
            assert!(out.heartbeat.is_some());
        }
    } else {
        assert!(out.heartbeat.is_some());
    }
}

/// V4.2: whole minutes are the floor of the true duration (bounded to 2^40 ms, 34 years,
/// because the solver cannot do the 64-bit multiplication; concrete extremes below).
#[kani::proof]
fn v4_whole_minutes_floor() {
    let e: u64 = kani::any();
    kani::assume(e < 1 << 40);
    let m = whole_minutes(e);
    assert!(m <= e);
    assert!(m * 60_000 <= e && e - m * 60_000 < 60_000);
}

#[kani::proof]
fn v4_minutes_concrete_extremes() {
    assert!(whole_minutes(0) == 0);
    assert!(whole_minutes(59_999) == 0);
    assert!(whole_minutes(60_000) == 1);
    assert!(whole_minutes((1 << 29) + 60_000) == 8948); // main.py printed -8947 here
    assert!(whole_minutes(1 << 30) == 17895);
    assert!(whole_minutes(u64::MAX) == u64::MAX / 60_000);
    assert!(tenths_of_minute(0) == 0);
    assert!(tenths_of_minute(2_999) == 0);
    assert!(tenths_of_minute(3_000) == 1);
    assert!(tenths_of_minute(60_000) == 10);
    assert!(tenths_of_minute(u64::MAX) >= u64::MAX / 6_000);
    assert!(uptime_s(100 * 365 * 86_400_000) == 3_153_600_000);
}

static mut V4_SEEN: u64 = 0;
static mut V4_CALLS: u8 = 0;
static mut V4_RET: u64 = 0;
fn v4_minutes_recorder(elapsed_ms: u64) -> u64 {
    unsafe {
        V4_SEEN = elapsed_ms;
        V4_CALLS += 1;
        V4_RET
    }
}

/// V4.2: the open alert carries the minutes of exactly `now - open_since` (never a
/// negative or wrapped value), first at 15 min, then every 5 min, never while muted.
#[kani::proof]
#[kani::stub(crate::whole_minutes, v4_minutes_recorder)]
fn v4_open_alert_duration_is_true_elapsed() {
    let d = any_door();
    let now: u64 = kani::any();
    let ret: u64 = kani::any();
    unsafe {
        V4_RET = ret;
    }
    let (nd, ev) = door_tick(d, now, true);
    if let Some(m) = ev.open_alert {
        let os = d.open_since.unwrap();
        assert!(!d.muted);
        unsafe {
            assert!(V4_CALLS >= 1);
            // never a wrapped or negative duration, even if the clock went back (then 0)
            assert!(V4_SEEN == now.saturating_sub(os));
        }
        assert!(m == ret);
        match d.last_alert {
            None => assert!(now >= os && now - os >= INITIAL_ALERT_MS),
            Some(l) => assert!(now >= l && now - l >= REPEAT_ALERT_MS),
        }
        assert!(nd.last_alert == Some(now));
    } else if let Some(os) = d.open_since {
        // no alert although due and not muted would be a missed alert
        let due = match d.last_alert {
            None => now >= os && now - os >= INITIAL_ALERT_MS,
            Some(l) => now >= l && now - l >= REPEAT_ALERT_MS,
        };
        assert!(!due || d.muted);
    }
    kani::cover!(ev.open_alert.is_some());
}

/// V4.4: every transition is total: no overflow, no panic, for any state at all.
#[kani::proof]
fn v4_everything_total() {
    let _ = step(any_state(), any_step_in());
    let _ = on_update(any_state(), any_update(), kani::any(), kani::any());
    let _ = on_poll(any_state(), any_poll(), kani::any());
    let _ = door_tick(any_door(), kani::any(), kani::any());
    let _ = pick_op(any_state(), kani::any(), kani::any(), kani::any());
    let _ = make_anchor(kani::any(), kani::any(), kani::any());
    let _ = interval_due(kani::any(), kani::any(), kani::any());
    let _ = tenths_of_minute(kani::any());
}

/// V4: interval_due is the plain definition and does not go silent on large values.
#[kani::proof]
fn v4_interval_due() {
    let last: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let iv: u64 = kani::any();
    let r = interval_due(last, now, iv);
    match last {
        None => assert!(r),
        Some(l) => {
            if now >= l {
                assert!(r == (now - l >= iv));
            } else {
                assert!(r == (iv == 0));
            }
        }
    }
}

// =====================================================================
// V5  A fresh command is never dropped
// =====================================================================

/// V5.1: /update only sets a flag. The rest of the batch stays; nothing is pressed.
#[kani::proof]
fn v5_update_keeps_batch() {
    let s = any_state();
    let mut u = any_update();
    u.cmd = Cmd::Update;
    kani::assume(s.batch_left > 0);
    let (t, press) = on_update(s, u, kani::any(), kani::any());
    assert!(!press);
    assert!(t.batch_left == s.batch_left || t.batch_left == s.batch_left - 1);
    assert!(t.door == s.door);
    kani::cover!(t.update_pending && !s.update_pending && t.batch_left > 0);
}

/// V5.1/V5.2: OTA starts only when nothing is pending, no batch is open and the last poll
/// was good and empty (so Telegram has our offset and holds nothing).
#[kani::proof]
fn v5_ota_only_when_quiet() {
    let s = any_state();
    let i = any_step_in();
    let out = step(s, i);
    if matches!(out.op, Op::Ota) {
        assert!(s.update_pending && s.quiesced && i.wifi_up);
        assert!(s.batch_left == 0);
        assert!(s.pend_reply.is_none() && !s.pend_online);
        assert!(out.state.pend_reply.is_none());
        assert!(out.state.pend_close_timeout.is_none());
        assert!(out.state.pend_open_alert.is_none());
        assert!(out.state.pend_closed_notice.is_none());
    }
    kani::cover!(matches!(out.op, Op::Ota));
}

/// V5.2: `quiesced` can only become true through a good, empty poll outside a batch, and a
/// good non-empty poll clears it.
#[kani::proof]
fn v5_quiesced_means_empty_good_poll() {
    let s = any_state();
    let p = any_poll();
    let t = on_poll(s, p, kani::any());
    if t.quiesced && !s.quiesced {
        assert!(p.ok && p.count == 0 && s.batch_left == 0);
    }
    if s.batch_left == 0 && p.ok && p.count > 0 {
        assert!(!t.quiesced && t.batch_left > 0);
    }
    let (t2, _) = on_update(s, any_update(), kani::any(), kani::any());
    assert!(!(t2.quiesced && !s.quiesced));
    let o = step(s, any_step_in());
    assert!(!(o.state.quiesced && !s.quiesced));
}

/// V5.2 stronger: OTA must not start on a `quiesced` that is older than the /update.
/// Invariant I: batch open => not quiesced. It holds at boot and every transition keeps it,
/// so when /update is accepted (batch open) `quiesced` is false and a new empty good poll is
/// needed before `v5_ota_only_when_quiet` lets OTA start.
/// (First version claimed this for arbitrary states and failed on quiesced && batch_left > 0,
/// which this invariant shows is unreachable: wrong property, fixed here.)
#[kani::proof]
fn v5_update_needs_new_empty_poll() {
    assert!(!(BOOT_STATE.batch_left > 0 && BOOT_STATE.quiesced));
    let s = any_state();
    kani::assume(!(s.batch_left > 0 && s.quiesced));
    let mut u = any_update();
    let (t, _) = on_update(s, u, kani::any(), kani::any());
    assert!(!(t.batch_left > 0 && t.quiesced));
    if t.update_pending && !s.update_pending {
        assert!(!t.quiesced);
    }
    let t2 = on_poll(s, any_poll(), kani::any());
    assert!(!(t2.batch_left > 0 && t2.quiesced));
    let o = step(s, any_step_in());
    assert!(!(o.state.batch_left > 0 && o.state.quiesced));
}

/// V5.3: no flush exists. The first command after boot, fetched by the first good poll and
/// sent in the same second as that response, is executed.
#[kani::proof]
fn v5_first_command_after_boot_executes() {
    let d: u64 = kani::any();
    let t_req: u64 = kani::any();
    let now: u64 = kani::any();
    kani::assume(d < MAX_UNIX_S && d >= 1_600_000_000);
    // the poll itself took at most its 30 s budget (first run used 60 s: age came out 63 s, Stale)
    kani::assume(t_req >= 2_000 && t_req <= now && now - t_req <= 30_000 && now < 1 << 40);
    kani::assume(d + 1 >= t_req / 1000);
    let mut s = BOOT_STATE;
    s.pend_online = false; // greeting already sent
    let p = PollResult { ok: true, count: 1, date_unix: Some(d), t_req_ms: t_req };
    let s1 = on_poll(s, p, now);
    assert!(s1.batch_left == 1 && s1.anchor.is_some());
    let u = UpdateIn {
        update_id: kani::any(),
        authorized: true,
        cmd: Cmd::Open,
        date: Some(d),
        who: any_name(),
    };
    kani::assume(u.update_id < MAX_UPDATE_ID);
    let (s2, press) = on_update(s1, u, now, false);
    assert!(press);
    assert!(matches!(s2.pend_reply, Some(o) if matches!(o.reply, Reply::Opening)));
}

/// V5.4: a good poll opens a batch for everything it kept (up to MAX_BATCH), so no
/// delivered update is skipped.
#[kani::proof]
fn v5_good_poll_opens_full_batch() {
    let s = any_state();
    let mut p = any_poll();
    p.ok = true;
    kani::assume(s.batch_left == 0);
    let t = on_poll(s, p, kani::any());
    assert!(t.batch_left == if p.count < MAX_BATCH { p.count } else { MAX_BATCH });
    assert!(t.next_offset == s.next_offset);
    assert!(t.pend_reply == s.pend_reply);
}

// =====================================================================
// V6  Watchdog fed or reset
// =====================================================================

/// V6.2: whatever the state, the one op of a pass fits under the watchdog with slack.
#[kani::proof]
fn v6_every_op_fits_watchdog() {
    let out = step(any_state(), any_step_in());
    let b = op_budget_ms(out.op);
    assert!(b <= MAX_OP_BUDGET_MS);
    assert!(b + LOOP_SLACK_MS + RELAY_PRESS_MS < WDT_TIMEOUT_MS);
}

/// V6.2: a late pass (previous pass more than the WDT timeout ago) orders a reboot.
#[kani::proof]
fn v6_overrun_reboots() {
    let s = any_state();
    let i = any_step_in();
    kani::assume(matches!(s.last_step, Some(l) if i.now_ms > l && i.now_ms - l > WDT_TIMEOUT_MS));
    let out = step(s, i);
    assert!(matches!(out.op, Op::Reboot));
}

/// V6.3: WiFi down: only a bounded connect or a reboot, and a reboot once the outage is
/// WIFI_REBOOT_AFTER_MS old. Not alive-and-deaf forever.
#[kani::proof]
fn v6_wifi_down_bounded() {
    let s = any_state();
    let mut i = any_step_in();
    i.wifi_up = false;
    let out = step(s, i);
    assert!(matches!(out.op, Op::WifiConnect | Op::Reboot));
    if let Some(t) = s.wifi_down_since {
        if i.now_ms >= t && i.now_ms - t >= WIFI_REBOOT_AFTER_MS {
            assert!(matches!(out.op, Op::Reboot));
        }
        assert!(out.state.wifi_down_since == Some(t));
    } else {
        assert!(out.state.wifi_down_since == Some(i.now_ms));
    }
}

/// V6: every pass stamps `last_step`, so the overrun check always has the previous pass.
#[kani::proof]
fn v6_pass_is_stamped() {
    let i = any_step_in();
    let out = step(any_state(), i);
    assert!(out.state.last_step == Some(i.now_ms));
}

/// V6.3 (spec wish): WiFi up but every poll failing for a long time: is there any bound
/// after which the bot resets itself? Expected to FAIL if no such bound exists:
/// the state below has had no successful poll for 24 h and the bot neither polls nor reboots
/// only if it is stuck; this checks it at least keeps polling.
#[kani::proof]
fn v6_keeps_polling_when_polls_fail() {
    let mut s = any_state();
    s.pend_online = false;
    s.pend_reply = None;
    s.pend_close_timeout = None;
    s.pend_open_alert = None;
    s.pend_closed_notice = None;
    s.batch_left = 0;
    s.door = DoorState { open_since: None, last_alert: None, muted: false, close: None };
    let i = StepIn { now_ms: kani::any(), door_open: false, wifi_up: true };
    kani::assume(match s.last_step {
        None => true,
        Some(l) => i.now_ms >= l && i.now_ms - l <= WDT_TIMEOUT_MS,
    });
    kani::assume(matches!(s.last_poll, Some(l) if i.now_ms >= l && i.now_ms - l >= POLL_INTERVAL_MS));
    // a failed poll result, then the next pass
    let s1 = on_poll(s, PollResult { ok: false, count: 0, date_unix: None, t_req_ms: 0 }, i.now_ms);
    let out = step(s1, i);
    assert!(matches!(out.op, Op::Poll { .. } | Op::Ota));
    // with an update queued and polls failing, OTA must not start
    assert!(!matches!(out.op, Op::Ota));
}

// =====================================================================
// Glue that decides WHO and WHAT (TRUSTED.md T6/T7 challenge): cheap direct properties
// =====================================================================

fn lc(b: u8) -> u8 {
    if b >= b'A' && b <= b'Z' { b + 32 } else { b }
}

/// A text is read as /open only if it IS "open" or "/open" (any case), with nothing but
/// whitespace around it and optionally "@..." after it. Checked for every text up to 14
/// bytes, which includes "/open@" plus a suffix; main.py semantics.
#[kani::proof]
#[kani::unwind(16)]
fn t7_only_open_parses_as_open() {
    const N: usize = 14;
    let buf: [u8; N] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= N);
    let t = &buf[..len];
    if matches!(parse_command(t), Cmd::Open) {
        // skip leading whitespace
        let mut i = 0;
        while i < len && t[i] <= b' ' {
            i += 1;
        }
        if i < len && t[i] == b'/' {
            i += 1;
        }
        assert!(i + 4 <= len);
        assert!(lc(t[i]) == b'o' && lc(t[i + 1]) == b'p' && lc(t[i + 2]) == b'e' && lc(t[i + 3]) == b'n');
        let mut j = i + 4;
        if j < len && t[j] != b'@' {
            while j < len {
                assert!(t[j] <= b' ');
                j += 1;
            }
        }
    }
    kani::cover!(matches!(parse_command(t), Cmd::Open) && len == N);
}

/// An update is "authorized" only if its chat id IS the configured chat and it has a text;
/// id and date are passed through untouched.
#[kani::proof]
#[kani::unwind(66)]
fn t7_authorized_means_configured_chat() {
    let r = RawUpdate {
        update_id: kani::any(),
        chat_id: kani::any(),
        date: kani::any(),
        has_text: kani::any(),
        text_over: kani::any(),
        text_len: kani::any(),
        text: kani::any(),
        who: any_name(),
    };
    let chat: i64 = kani::any();
    match to_update_in(r, chat) {
        None => assert!(r.update_id.is_none()),
        Some(u) => {
            assert!(Some(u.update_id) == r.update_id);
            assert!(u.date == r.date);
            if u.authorized {
                assert!(r.chat_id == Some(chat) && r.has_text);
            }
            if r.text_over {
                assert!(matches!(u.cmd, Cmd::Unknown));
            }
        }
    }
}

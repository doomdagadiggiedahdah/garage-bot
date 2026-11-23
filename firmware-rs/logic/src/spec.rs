//! Executable specifications. Every function in fns/ is proven equal to its
//! `spec_*` twin here, and the top-level properties in props.rs are proven
//! about these. This file is NOT compiled into the firmware.
//!
//! Written for clarity, not speed. Differential tests (difftest/) compare these
//! against the MicroPython firmware's behaviour.
// @prelude
use super::*;

// ================================================================ time
pub fn spec_uptime_s(now_ms: u64) -> u64 {
    now_ms / 1000
}

/// "At least `interval` ms have passed since `last`" (never ran = due).
pub fn spec_due(last: Option<u64>, now: u64, interval: u64) -> bool {
    match last {
        None => true,
        Some(t) => now.saturating_sub(t) >= interval,
    }
}

/// Whole minutes: the same number as `elapsed_ms / 60_000`. Written as two
/// divisions because the SAT solver can compare two copies of this form (about
/// 6 minutes) but not two copies of a single division by 60_000.
pub fn spec_whole_minutes(elapsed_ms: u64) -> u64 {
    elapsed_ms / 1000 / 60
}

/// Minutes in tenths, rounded half up: 89_999 ms -> 15 (1.5 min).
/// The same number as `(elapsed_ms + 3_000) / 6_000`, split for the solver as above.
pub fn spec_tenths(elapsed_ms: u64) -> u64 {
    elapsed_ms.saturating_add(3_000) / 1000 / 6
}

// ================================================================ trustworthy age
/// Anchor from an HTTP `Date` header (unix seconds `date`) of a response whose
/// request was sent at or after monotonic `t_req_ms`; `now` is when it is stored.
pub fn spec_anchor(date: u64, t_req_ms: u64, now: u64) -> Option<Anchor> {
    if date < MAX_UNIX_S && t_req_ms <= now && date + 1 >= t_req_ms / 1000 {
        Some(Anchor { unix_s: date + 1, mono_ms: t_req_ms })
    } else {
        None
    }
}

pub fn spec_anchor_usable(k: Anchor, now: u64) -> bool {
    k.unix_s <= MAX_UNIX_S && now >= k.mono_ms && now - k.mono_ms <= ANCHOR_MAX_AGE_MS && k.unix_s >= k.mono_ms / 1000
}

/// Upper bound on the current unix time in seconds. Only call when usable.
pub fn spec_now_unix(k: Anchor, now: u64) -> u64 {
    k.unix_s + (now - k.mono_ms) / 1000 + NOW_SLACK_S
}

/// Upper bound on the unix second of this boot, from the first anchor.
/// One extra second covers a monotonic clock that ran fast by up to 1 s since boot.
pub fn spec_boot_unix(k: Anchor) -> u64 {
    k.unix_s.saturating_add(1).saturating_sub(k.mono_ms / 1000)
}

pub fn spec_age(anchor: Option<Anchor>, boot_unix: Option<u64>, now: u64, date: Option<u64>) -> Age {
    match (anchor, boot_unix, date) {
        (Some(k), Some(b), Some(d)) => {
            if !spec_anchor_usable(k, now) {
                return Age::Unknown;
            }
            let nu = spec_now_unix(k, now);
            if d > nu {
                Age::Unknown
            } else if d < b {
                Age::PreBoot
            } else if nu - d > STALE_AFTER_S {
                Age::Stale(nu - d)
            } else {
                Age::Fresh(nu - d)
            }
        }
        _ => Age::Unknown,
    }
}

// ================================================================ commands
pub fn spec_is_guarded(cmd: Cmd) -> bool {
    matches!(cmd, Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Update)
}

pub const NO_DECISION: Decision = Decision { press: false, reply: None, arm_close: false, mute: false, queue_update: false };

pub fn spec_decide(cmd: Cmd, age: Age, door_open: bool) -> Decision {
    let say = |r: Reply| Decision { reply: Some(r), ..NO_DECISION };
    match cmd {
        Cmd::Unknown => NO_DECISION,
        Cmd::Status => say(Reply::Status { open: door_open }),
        Cmd::Diag => say(Reply::Diag),
        Cmd::Version => say(Reply::Version),
        Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Update => match age {
            // Not fresh (no usable clock, sent before this boot, or too old): dropped
            // silently. No press, no reply; on_update still moves the offset past it.
            Age::Unknown | Age::PreBoot | Age::Stale(_) => NO_DECISION,
            Age::Fresh(_) => match cmd {
                Cmd::Open => {
                    if door_open {
                        say(Reply::AlreadyOpen)
                    } else {
                        Decision { press: true, ..say(Reply::Opening) }
                    }
                }
                Cmd::Close => {
                    if door_open {
                        Decision { press: true, arm_close: true, ..say(Reply::Closing) }
                    } else {
                        say(Reply::AlreadyClosed)
                    }
                }
                Cmd::Silence => Decision { mute: true, ..say(Reply::Muted) },
                _ => Decision { queue_update: true, ..say(Reply::UpdateQueued) },
            },
        },
    }
}

/// Process one update of the current batch. Returns the new state and whether
/// the relay must be pressed NOW (the shell presses iff this is true).
pub fn spec_on_update(s: State, u: UpdateIn, now: u64, door_open: bool) -> (State, bool) {
    if s.batch_left == 0 || s.pend_reply.is_some() {
        return (s, false);
    }
    let mut n = s;
    n.batch_left = s.batch_left - 1;
    let id_ok = u.update_id < MAX_UPDATE_ID && u.update_id >= s.next_offset;
    if id_ok {
        n.next_offset = u.update_id + 1;
    }
    if !(id_ok && u.authorized) {
        return (n, false);
    }
    let d = spec_decide(u.cmd, spec_age(s.anchor, s.boot_unix, now, u.date), door_open);
    n.pend_reply = match d.reply {
        Some(r) => Some(Outgoing { who: u.who, reply: r }),
        None => None,
    };
    if d.arm_close {
        n.door.close = Some(CloseReq { at: now, who: u.who, alerted: false });
    }
    if d.mute {
        n.door.muted = true;
    }
    if d.queue_update {
        n.update_pending = true;
    }
    (n, d.press)
}

pub fn spec_on_poll(s: State, p: PollResult, now: u64) -> State {
    if s.batch_left > 0 {
        return s;
    }
    let mut n = s;
    if !p.ok {
        n.quiesced = false;
        return n;
    }
    n.last_good_poll = Some(now);
    n.batch_left = if p.count > MAX_BATCH { MAX_BATCH } else { p.count };
    n.quiesced = p.count == 0;
    if let Some(d) = p.date_unix {
        if let Some(k) = spec_anchor(d, p.t_req_ms, now) {
            n.anchor = Some(k);
            if s.boot_unix.is_none() {
                n.boot_unix = Some(spec_boot_unix(k));
            }
        }
    }
    n
}

// ================================================================ door
/// The "door closed after X.Y minutes" notice: only when a bot close was
/// pending and the door was open at least CLOSED_NOTICE_MIN_MS.
pub fn spec_closed_notice(d: DoorState, now: u64) -> Option<ClosedNotice> {
    if let (Some(s0), Some(c)) = (d.open_since, d.close) {
        let e = now.saturating_sub(s0);
        if e >= CLOSED_NOTICE_MIN_MS {
            return Some(ClosedNotice { who: c.who, tenths: spec_tenths(e) });
        }
    }
    None
}

/// An open alert is due: the door is known open, not muted, and either no
/// alert was sent and INITIAL_ALERT_MS have passed, or REPEAT_ALERT_MS have
/// passed since the last one.
pub fn spec_open_alert_due(d: DoorState, now: u64) -> bool {
    match d.open_since {
        None => false,
        Some(s0) => {
            let due = match d.last_alert {
                None => now.saturating_sub(s0) >= INITIAL_ALERT_MS,
                Some(a) => now.saturating_sub(a) >= REPEAT_ALERT_MS,
            };
            due && !d.muted
        }
    }
}

/// A pending bot close has timed out and was not reported yet.
pub fn spec_close_timeout_due(c: CloseReq, now: u64) -> bool {
    !c.alerted && now.saturating_sub(c.at) >= CLOSE_TIMEOUT_MS
}

pub fn spec_door(d: DoorState, now: u64, open: bool) -> (DoorState, DoorEvents) {
    let mut ev = DoorEvents { open_alert: None, close_timeout: None, closed_notice: None };
    if !open {
        ev.closed_notice = spec_closed_notice(d, now);
        return (DOOR_IDLE, ev);
    }
    let mut n = d;
    match d.open_since {
        None => {
            n.open_since = Some(now);
            n.last_alert = None;
            n.muted = false;
        }
        Some(s0) => {
            if spec_open_alert_due(d, now) {
                ev.open_alert = Some(spec_whole_minutes(now.saturating_sub(s0)));
                n.last_alert = Some(now);
            }
        }
    }
    if let Some(c) = d.close {
        if spec_close_timeout_due(c, now) {
            ev.close_timeout = Some(c.who);
            n.close = Some(CloseReq { at: c.at, who: c.who, alerted: true });
        }
    }
    (n, ev)
}

// ================================================================ step
pub fn spec_op_budget_ms(op: Op) -> u64 {
    match op {
        Op::Reboot => 0,
        Op::WifiConnect => 20_000,
        Op::Send(_) => 30_000,
        Op::Poll { .. } => 30_000,
        Op::NextUpdate => 1_000,
        Op::Ota => MAX_OP_BUDGET_MS,
        Op::Idle => 1_000,
    }
}

/// Which op to do, given the state AFTER door/wifi bookkeeping of this pass.
/// No successful poll for DEAF_REBOOT_AFTER_MS, counted from boot (time 0)
/// when there was none yet.
pub fn spec_deaf(last_good_poll: Option<u64>, now: u64) -> bool {
    let since = match last_good_poll {
        Some(t) => t,
        None => 0,
    };
    now.saturating_sub(since) >= DEAF_REBOOT_AFTER_MS
}

pub fn spec_pick_op(n: State, now: u64, overrun: bool, wifi_up: bool) -> Op {
    if overrun {
        return Op::Reboot;
    }
    if spec_deaf(n.last_good_poll, now) {
        return Op::Reboot;
    }
    if !wifi_up {
        return match n.wifi_down_since {
            Some(t) if now.saturating_sub(t) >= WIFI_REBOOT_AFTER_MS => Op::Reboot,
            _ => Op::WifiConnect,
        };
    }
    if n.pend_online {
        return Op::Send(Msg::Online);
    }
    if let Some(o) = n.pend_reply {
        return Op::Send(Msg::Reply(o));
    }
    if let Some(w) = n.pend_close_timeout {
        return Op::Send(Msg::CloseTimeout { who: w });
    }
    if let Some(m) = n.pend_open_alert {
        return Op::Send(Msg::OpenAlert { minutes: m });
    }
    if let Some(c) = n.pend_closed_notice {
        return Op::Send(Msg::ClosedAfter(c));
    }
    if n.batch_left > 0 {
        return Op::NextUpdate;
    }
    if n.update_pending && n.quiesced {
        return Op::Ota;
    }
    if spec_due(n.last_poll, now, POLL_INTERVAL_MS) {
        return Op::Poll { offset: n.next_offset };
    }
    Op::Idle
}

/// The previous pass was more than WDT_TIMEOUT_MS ago, or the clock went back.
pub fn spec_overrun(last_step: Option<u64>, now: u64) -> bool {
    match last_step {
        // a clock that went back is treated like a missed watchdog: reboot
        Some(t) => now < t || now - t > WDT_TIMEOUT_MS,
        None => false,
    }
}

/// Put this pass's door events into the pending slots. With the door closed,
/// first drop what is only worth saying about an open door.
pub fn spec_queue_door_events(s: State, ev: DoorEvents, door_open: bool) -> State {
    let mut n = s;
    if !door_open {
        // nothing about an open door is worth saying once it is closed
        n.pend_open_alert = None;
        n.pend_close_timeout = None;
    }
    if let Some(m) = ev.open_alert {
        n.pend_open_alert = Some(m);
    }
    if let Some(w) = ev.close_timeout {
        n.pend_close_timeout = Some(w);
    }
    if let Some(c) = ev.closed_notice {
        n.pend_closed_notice = Some(c);
    }
    n
}

/// Since when WiFi is down: nothing while it is up, else the first time it was seen down.
pub fn spec_wifi_track(since: Option<u64>, now: u64, wifi_up: bool) -> Option<u64> {
    if wifi_up {
        None
    } else {
        match since {
            Some(t) => Some(t),
            None => Some(now),
        }
    }
}

/// The op was chosen: clear the pending slot it serves (or stamp the poll time, or clear the update flag).
pub fn spec_op_done(s: State, op: Op, now: u64) -> State {
    let mut n = s;
    match op {
        Op::Send(Msg::Online) => n.pend_online = false,
        Op::Send(Msg::Reply(_)) => n.pend_reply = None,
        Op::Send(Msg::CloseTimeout { .. }) => n.pend_close_timeout = None,
        Op::Send(Msg::OpenAlert { .. }) => n.pend_open_alert = None,
        Op::Send(Msg::ClosedAfter(_)) => n.pend_closed_notice = None,
        Op::Poll { .. } => n.last_poll = Some(now),
        Op::Ota => n.update_pending = false,
        _ => {}
    }
    n
}

/// One loop pass: door monitoring, bookkeeping, and the choice of the one op.
pub fn spec_step(s: State, i: StepIn) -> StepOut {
    let now = i.now_ms;
    let overrun = spec_overrun(s.last_step, now);
    let (door, ev) = spec_door(s.door, now, i.door_open);
    let mut n = s;
    n.last_step = Some(now);
    n.door = door;
    n = spec_queue_door_events(n, ev, i.door_open);
    n.wifi_down_since = spec_wifi_track(s.wifi_down_since, now, i.wifi_up);
    let hb = spec_due(s.last_heartbeat, now, HEARTBEAT_INTERVAL_MS);
    if hb {
        n.last_heartbeat = Some(now);
    }
    let op = spec_pick_op(n, now, overrun, i.wifi_up);
    let n = spec_op_done(n, op, now);
    StepOut { state: n, op, heartbeat: if hb { Some(spec_uptime_s(now)) } else { None } }
}

// ================================================================ text
pub fn spec_text_push(t: Text, s: &[u8]) -> Text {
    if t.overflow || t.len as usize > TEXT_CAP || s.len() > TEXT_CAP - t.len as usize {
        return Text { overflow: true, ..t };
    }
    let mut r = t;
    let mut i = 0;
    while i < s.len() {
        r.buf[t.len as usize + i] = s[i];
        i += 1;
    }
    r.len = t.len + s.len() as u16;
    r
}

/// Decimal digits of `v`, right-aligned: the digits are `.0[.1..20]`.
/// One decimal step: quotient and last digit.
pub fn spec_div10(v: u64) -> (u64, u8) {
    (v / 10, (v % 10) as u8)
}

/// Quotient and remainder by 60 (minutes and seconds, or hours and minutes).
pub fn spec_div60(v: u64) -> (u64, u64) {
    (v / 60, v % 60)
}

/// Decimal digits of `v`, right-aligned in 20 bytes; the digits are `b[i..]`,
/// the bytes before `i` are 0. No leading zeros; 0 is the one digit "0".
pub fn spec_dec(v: u64) -> ([u8; 20], usize) {
    let mut b = [0u8; 20];
    let mut i = 20;
    let mut x = v;
    loop {
        i -= 1;
        let (q, r) = spec_div10(x);
        b[i] = b'0' + r;
        x = q;
        if x == 0 {
            break;
        }
    }
    (b, i)
}

pub fn spec_text_push_u64(t: Text, v: u64) -> Text {
    let (b, i) = spec_dec(v);
    spec_text_push(t, &b[i..])
}

pub fn spec_text_push_i64(t: Text, v: i64) -> Text {
    if v < 0 {
        spec_text_push_u64(spec_text_push(t, b"-"), v.unsigned_abs())
    } else {
        spec_text_push_u64(t, v as u64)
    }
}

/// Right-aligned in `width` columns with spaces (Python `{v:>width}`); never truncates.
pub fn spec_text_push_padded(t: Text, v: u64, width: u8) -> Text {
    let (b, i) = spec_dec(v);
    let digits = 20 - i;
    let mut r = t;
    let mut k = digits;
    while k < width as usize {
        r = spec_text_push(r, b" ");
        k += 1;
    }
    spec_text_push(r, &b[i..])
}

pub fn spec_hex(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'a' + (n - 10)
    }
}

/// One byte escaped for the inside of a JSON string: the bytes and how many (1, 2 or 6).
/// `"` -> `\"`, `\` -> `\\`, 0x0a -> `\n`, other bytes below 0x20 -> `\u00xx`
/// (lowercase hex), everything else unchanged. Unused bytes are 0.
pub fn spec_json_escape(c: u8) -> ([u8; 6], u8) {
    if c == b'"' {
        ([b'\\', b'"', 0, 0, 0, 0], 2)
    } else if c == b'\\' {
        ([b'\\', b'\\', 0, 0, 0, 0], 2)
    } else if c == b'\n' {
        ([b'\\', b'n', 0, 0, 0, 0], 2)
    } else if c < 0x20 {
        ([b'\\', b'u', b'0', b'0', spec_hex(c >> 4), spec_hex(c & 15)], 6)
    } else {
        ([c, 0, 0, 0, 0, 0], 1)
    }
}

/// Append `s` as the inside of a JSON string (no surrounding quotes).
/// `"` -> `\"`, `\` -> `\\`, 0x0a -> `\n`, other bytes below 0x20 -> `\u00xx`
/// (lowercase hex), everything else unchanged (UTF-8 passes through).
pub fn spec_text_push_json(t: Text, s: &[u8]) -> Text {
    let mut r = t;
    let mut i = 0;
    while i < s.len() {
        let (e, n) = spec_json_escape(s[i]);
        r = spec_text_push(r, &e[..n as usize]);
        i += 1;
    }
    r
}

pub fn spec_name_ok(n: &Name) -> bool {
    if n.len as usize > NAME_CAP {
        return false;
    }
    let mut i = 0;
    while i < NAME_CAP {
        let c = n.bytes[i];
        if i < n.len as usize {
            if !spec_is_name_byte(c) {
                return false;
            }
        } else if c != 0 {
            return false;
        }
        i += 1;
    }
    true
}

pub fn spec_is_name_byte(c: u8) -> bool {
    (c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z') || (c >= b'0' && c <= b'9') || c == b'_'
}

/// "@name " or nothing when the name is empty (or malformed).
pub fn spec_push_mention(t: Text, who: &Name) -> Text {
    if who.len == 0 || who.len as usize > NAME_CAP {
        return t;
    }
    let r = spec_text_push(t, b"@");
    let r = spec_text_push(r, &who.bytes[..who.len as usize]);
    spec_text_push(r, b" ")
}

pub fn open_closed(open: bool) -> &'static [u8] {
    if open {
        b"OPEN"
    } else {
        b"CLOSED"
    }
}

/// `@name Door closed after X.Y minutes.` (`tenths` = minutes in tenths).
pub fn spec_render_closed_after(c: ClosedNotice) -> Text {
    let p = spec_text_push;
    let (q, r) = spec_div10(c.tenths);
    let t = spec_push_mention(Text::EMPTY, &c.who);
    let t = p(t, b"Door closed after ");
    let t = spec_text_push_u64(t, q);
    let t = p(t, b".");
    let t = spec_text_push_u64(t, r as u64);
    p(t, b" minutes.")
}

/// The reply to the debug command.
pub fn spec_render_diag(version: &[u8], d: Diag) -> Text {
    let p = spec_text_push;
    let (m, s) = spec_div60(d.uptime_s);
    let t = p(Text::EMPTY, b"System Info:\nVersion: ");
    let t = p(t, version);
    let t = p(t, b"\nUptime: ");
    let t = spec_text_push_u64(t, m);
    let t = p(t, b"m ");
    let t = spec_text_push_u64(t, s);
    let t = p(t, b"s\nFree memory: ");
    let t = spec_text_push_u64(t, d.free_mem as u64);
    let t = p(t, b" bytes\nLoop count: ");
    let t = spec_text_push_u64(t, d.stats.loops);
    let t = p(t, b"\nDoor: ");
    let t = p(t, open_closed(d.door_open));
    let t = p(t, b"\nHTTP: ok=");
    let t = spec_text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b" fail=");
    let t = spec_text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b" sock_err=");
    spec_text_push_u64(t, d.stats.sock_err as u64)
}

/// The text of a reply to a command.
pub fn spec_render_reply(o: Outgoing, version: &[u8], d: Diag) -> Text {
    let p = spec_text_push;
    let t = Text::EMPTY;
    match o.reply {
        Reply::Status { open } => p(p(t, b"Door is "), open_closed(open)),
        Reply::AlreadyOpen => p(t, b"Door is already open!"),
        Reply::Opening => p(spec_push_mention(t, &o.who), b"Opening door..."),
        Reply::AlreadyClosed => p(t, b"Door is already closed!"),
        Reply::Closing => p(spec_push_mention(t, &o.who), b"Closing door..."),
        Reply::Muted => p(t, b"Alerts muted until door closes."),
        Reply::UpdateQueued => p(t, b"Update queued. It installs once pending commands are done."),
        Reply::Diag => spec_render_diag(version, d),
        Reply::Version => p(p(t, b"GarageBot "), version),
    }
}

/// The Telegram message text for `m`.
/// `notify`: the mention string for open-door alerts, verbatim (e.g. `@alice @bob`).
/// `version`: firmware version, verbatim.
pub fn spec_render_msg(m: Msg, notify: &[u8], version: &[u8], d: Diag) -> Text {
    let p = spec_text_push;
    let t = Text::EMPTY;
    match m {
        Msg::Online => {
            let t = p(t, b"Garage door bot is online!\nVersion: ");
            let t = p(t, version);
            p(t, b"\n\nType 'help' for commands.")
        }
        Msg::CloseTimeout { who } => p(spec_push_mention(t, &who), b"WARNING: Door did not close! It may be blocked."),
        Msg::OpenAlert { minutes } => {
            let t = p(t, notify);
            let t = p(t, b" ALERT: Garage door has been open for ");
            let t = spec_text_push_u64(t, minutes);
            p(t, b" minutes!")
        }
        Msg::ClosedAfter(c) => spec_render_closed_after(c),
        Msg::Reply(o) => spec_render_reply(o, version, d),
    }
}

/// JSON body for sendMessage: `{"chat_id":N,"text":"..."}`.
pub fn spec_render_send_body(chat_id: i64, text: &Text) -> Text {
    if text.overflow || text.len as usize > TEXT_CAP {
        return Text { overflow: true, ..Text::EMPTY };
    }
    let t = spec_text_push(Text::EMPTY, b"{\"chat_id\":");
    let t = spec_text_push_i64(t, chat_id);
    let t = spec_text_push(t, b",\"text\":\"");
    let t = spec_text_push_json(t, &text.buf[..text.len as usize]);
    spec_text_push(t, b"\"}")
}

/// Path after `https://api.telegram.org/bot<token>/`.
pub fn spec_render_poll_path(offset: u64) -> Text {
    let t = spec_text_push(Text::EMPTY, b"getUpdates?offset=");
    let t = spec_text_push_u64(t, offset);
    spec_text_push(t, b"&timeout=2&limit=8")
}

/// `[{uptime:>6}s] [{level:>5}] [mem:{free:>6}] {msg}`
pub fn spec_render_log(uptime_s: u64, level: Level, free_mem: u32, msg: &[u8]) -> Text {
    let t = spec_text_push(Text::EMPTY, b"[");
    let t = spec_text_push_padded(t, uptime_s, 6);
    let t = spec_text_push(t, b"s] [");
    let lv: &[u8] = match level {
        Level::Info => b" INFO",
        Level::Warn => b" WARN",
        Level::Error => b"ERROR",
    };
    let t = spec_text_push(t, lv);
    let t = spec_text_push(t, b"] [mem:");
    let t = spec_text_push_padded(t, free_mem as u64, 6);
    let t = spec_text_push(t, b"] ");
    spec_text_push(t, msg)
}

/// `HEARTBEAT: door=OPEN, uptime=3m, loops=1, mem=2, rssi=-60, http_ok=1, http_fail=0, sock_err=0`
pub fn spec_render_heartbeat(d: Diag) -> Text {
    let p = spec_text_push;
    let t = p(Text::EMPTY, b"HEARTBEAT: door=");
    let t = p(t, open_closed(d.door_open));
    let t = p(t, b", uptime=");
    let t = spec_text_push_u64(t, spec_div60(d.uptime_s).0);
    let t = p(t, b"m, loops=");
    let t = spec_text_push_u64(t, d.stats.loops);
    let t = p(t, b", mem=");
    let t = spec_text_push_u64(t, d.free_mem as u64);
    let t = p(t, b", rssi=");
    let t = spec_text_push_i64(t, d.rssi as i64);
    let t = p(t, b", http_ok=");
    let t = spec_text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b", http_fail=");
    let t = spec_text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b", sock_err=");
    spec_text_push_u64(t, d.stats.sock_err as u64)
}

/// `HEAP {uptime_min},{mem},{loops},{http_ok},{http_fail},{sock_err}`
pub fn spec_render_heap(d: Diag) -> Text {
    let p = spec_text_push;
    let t = p(Text::EMPTY, b"HEAP ");
    let t = spec_text_push_u64(t, spec_div60(d.uptime_s).0);
    let t = p(t, b",");
    let t = spec_text_push_u64(t, d.free_mem as u64);
    let t = p(t, b",");
    let t = spec_text_push_u64(t, d.stats.loops);
    let t = p(t, b",");
    let t = spec_text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b",");
    let t = spec_text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b",");
    spec_text_push_u64(t, d.stats.sock_err as u64)
}

// ================================================================ parsing
/// Python `str.strip()` whitespace, ASCII part.
pub fn spec_is_ws(c: u8) -> bool {
    (c >= 0x09 && c <= 0x0d) || (c >= 0x1c && c <= 0x20)
}

pub fn spec_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c + 32
    } else {
        c
    }
}

/// `t` lowercased equals `w` (w is lowercase).
pub fn spec_word_is(t: &[u8], w: &[u8]) -> bool {
    if t.len() != w.len() {
        return false;
    }
    let mut i = 0;
    while i < t.len() {
        if spec_lower(t[i]) != w[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// main.py: `text.lower().strip().split("@")[0]`, then exact match against the word lists.
/// Strip whitespace at both ends; cut at the first '@' (no second strip); compare.
pub fn spec_parse_command(t: &[u8]) -> Cmd {
    let mut a = 0;
    while a < t.len() && spec_is_ws(t[a]) {
        a += 1;
    }
    let mut e = t.len();
    while e > a && spec_is_ws(t[e - 1]) {
        e -= 1;
    }
    let mut k = a;
    while k < e && t[k] != b'@' {
        k += 1;
    }
    let w = &t[a..k];
    let is = |x: &[u8]| spec_word_is(w, x);
    if is(b"status") || is(b"/status") {
        Cmd::Status
    } else if is(b"open") || is(b"/open") {
        Cmd::Open
    } else if is(b"close") || is(b"/close") {
        Cmd::Close
    } else if is(b"silence") || is(b"/silence") || is(b"quiet") || is(b"stop") || is(b"mute") {
        Cmd::Silence
    } else if is(b"update") || is(b"/update") {
        Cmd::Update
    } else if is(b"debug") || is(b"/debug") || is(b"info") || is(b"/info") {
        Cmd::Diag
    } else if is(b"version") || is(b"/version") {
        Cmd::Version
    } else {
        Cmd::Unknown
    }
}

pub fn spec_is_digit(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

/// 1..=19 ASCII digits -> value. Anything else -> None.
pub fn spec_parse_u64(s: &[u8]) -> Option<u64> {
    if s.is_empty() || s.len() > 19 {
        return None;
    }
    let mut v: u64 = 0;
    let mut i = 0;
    while i < s.len() {
        if !spec_is_digit(s[i]) {
            return None;
        }
        v = v * 10 + (s[i] - b'0') as u64;
        i += 1;
    }
    Some(v)
}

/// Optional '-', then 1..=18 ASCII digits -> value. Anything else -> None.
pub fn spec_parse_i64(s: &[u8]) -> Option<i64> {
    let neg = !s.is_empty() && s[0] == b'-';
    let d = if neg { &s[1..] } else { s };
    if d.len() > 18 {
        return None;
    }
    match spec_parse_u64(d) {
        Some(v) => Some(if neg { -(v as i64) } else { v as i64 }),
        None => None,
    }
}

pub fn spec_is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn spec_days_in_month(y: u64, m: u64) -> u64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if spec_is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Days from 1970-01-01 to y-m-d (proleptic Gregorian). Needs y >= 1970, 1 <= m <= 12, d >= 1.
pub fn spec_days_from_civil(y: u64, m: u64, d: u64) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn two(s: &[u8], i: usize) -> Option<u64> {
    if spec_is_digit(s[i]) && spec_is_digit(s[i + 1]) {
        Some(((s[i] - b'0') * 10 + (s[i + 1] - b'0')) as u64)
    } else {
        None
    }
}

/// HTTP `Date` header in IMF-fixdate form, exactly 29 bytes:
/// `Sun, 06 Nov 1994 08:49:37 GMT` -> unix seconds. Anything else -> None.
/// The day name is not checked beyond being three bytes. Year 1970..=9999.
pub fn spec_http_date(s: &[u8]) -> Option<u64> {
    if s.len() != 29 {
        return None;
    }
    if s[3] != b',' || s[4] != b' ' || s[7] != b' ' || s[11] != b' ' || s[16] != b' ' || s[19] != b':' || s[22] != b':' || s[25] != b' ' {
        return None;
    }
    if s[26] != b'G' || s[27] != b'M' || s[28] != b'T' {
        return None;
    }
    let day = two(s, 5)?;
    let mon: u64 = match &s[8..11] {
        b"Jan" => 1,
        b"Feb" => 2,
        b"Mar" => 3,
        b"Apr" => 4,
        b"May" => 5,
        b"Jun" => 6,
        b"Jul" => 7,
        b"Aug" => 8,
        b"Sep" => 9,
        b"Oct" => 10,
        b"Nov" => 11,
        b"Dec" => 12,
        _ => return None,
    };
    let year = two(s, 12)? * 100 + two(s, 14)?;
    let hh = two(s, 17)?;
    let mm = two(s, 20)?;
    let ss = two(s, 23)?;
    if year < 1970 || day < 1 || day > spec_days_in_month(year, mon) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    Some(spec_days_from_civil(year, mon, day) * 86_400 + hh * 3_600 + mm * 60 + ss)
}

// ================================================================ JSON scan
pub fn spec_hex_val(b: u8) -> Option<u8> {
    if b >= b'0' && b <= b'9' {
        Some(b - b'0')
    } else if b >= b'a' && b <= b'f' {
        Some(b - b'a' + 10)
    } else if b >= b'A' && b <= b'F' {
        Some(b - b'A' + 10)
    } else {
        None
    }
}

pub fn spec_is_scalar_byte(b: u8) -> bool {
    (b >= b'0' && b <= b'9') || (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') || b == b'+' || b == b'-' || b == b'.'
}

/// Byte-at-a-time JSON tokenizer.
pub fn spec_lex_step(lx: Lex, b: u8) -> (Lex, Tok) {
    let bad = (Lex { mode: LexMode::Err, u_left: 0, u_acc: 0 }, Tok::Bad);
    let to = |mode: LexMode, t: Tok| (Lex { mode, u_left: 0, u_acc: 0 }, t);
    match lx.mode {
        LexMode::Err => bad,
        LexMode::Value => match b {
            b'{' => to(LexMode::Value, Tok::ObjOpen),
            b'}' => to(LexMode::Value, Tok::ObjClose),
            b'[' => to(LexMode::Value, Tok::ArrOpen),
            b']' => to(LexMode::Value, Tok::ArrClose),
            b':' => to(LexMode::Value, Tok::Colon),
            b',' => to(LexMode::Value, Tok::Comma),
            b' ' | b'\t' | b'\n' | b'\r' => to(LexMode::Value, Tok::Skip),
            b'"' => to(LexMode::Str, Tok::StrStart),
            _ => {
                if spec_is_scalar_byte(b) {
                    to(LexMode::Value, Tok::Scalar(b))
                } else {
                    bad
                }
            }
        },
        LexMode::Str => match b {
            b'"' => to(LexMode::Value, Tok::StrEnd),
            b'\\' => to(LexMode::Esc, Tok::Skip),
            _ => {
                if b < 0x20 {
                    bad
                } else {
                    to(LexMode::Str, Tok::StrByte(b))
                }
            }
        },
        LexMode::Esc => match b {
            b'"' | b'\\' | b'/' => to(LexMode::Str, Tok::StrByte(b)),
            b'b' => to(LexMode::Str, Tok::StrByte(0x08)),
            b'f' => to(LexMode::Str, Tok::StrByte(0x0c)),
            b'n' => to(LexMode::Str, Tok::StrByte(0x0a)),
            b'r' => to(LexMode::Str, Tok::StrByte(0x0d)),
            b't' => to(LexMode::Str, Tok::StrByte(0x09)),
            b'u' => (Lex { mode: LexMode::Uni, u_left: 4, u_acc: 0 }, Tok::Skip),
            _ => bad,
        },
        LexMode::Uni => match spec_hex_val(b) {
            None => bad,
            Some(v) => {
                if lx.u_left == 0 || lx.u_left > 4 {
                    return bad;
                }
                let acc = (lx.u_acc & 0x0fff) * 16 + v as u16;
                if lx.u_left == 1 {
                    // code points above 0x7f (and surrogates) become '?': no command contains them
                    to(LexMode::Str, Tok::StrByte(if acc < 0x80 { acc as u8 } else { b'?' }))
                } else {
                    (Lex { mode: LexMode::Uni, u_left: lx.u_left - 1, u_acc: acc }, Tok::Skip)
                }
            }
        },
    }
}

/// Which known key the bytes spell, for an object at path level `on`
/// (1 root, 3 update, 4 message, 5 chat or from).
pub fn spec_classify_key(on: u8, in_from: bool, k: &[u8]) -> Key {
    match on {
        1 => match k {
            b"ok" => Key::Ok,
            b"result" => Key::Result,
            _ => Key::Other,
        },
        3 => match k {
            b"update_id" => Key::UpdateId,
            b"message" => Key::Message,
            _ => Key::Other,
        },
        4 => match k {
            b"date" => Key::Date,
            b"text" => Key::Text,
            b"chat" => Key::Chat,
            b"from" => Key::From,
            _ => Key::Other,
        },
        5 => {
            if !in_from && k == b"id" {
                Key::Id
            } else if in_from && k == b"username" {
                Key::Username
            } else {
                Key::Other
            }
        }
        _ => Key::Other,
    }
}

pub fn spec_walk_ok(w: &Walk) -> bool {
    w.on <= w.depth && w.on <= 5 && w.depth <= MAX_DEPTH && w.tmp_len as usize <= TMP_CAP
}

/// A username from captured bytes: valid charset and length, else EMPTY.
pub fn spec_name_from(s: &[u8], over: bool) -> Name {
    if over || s.is_empty() || s.len() > NAME_CAP {
        return Name::EMPTY;
    }
    let mut n = Name::EMPTY;
    let mut i = 0;
    while i < s.len() {
        if !spec_is_name_byte(s[i]) {
            return Name::EMPTY;
        }
        n.bytes[i] = s[i];
        i += 1;
    }
    n.len = s.len() as u8;
    n
}

pub fn tmp_reset(mut n: Walk) -> Walk {
    n.cap = Cap::None;
    n.tmp_len = 0;
    n.tmp_over = false;
    n
}

pub fn spec_walk_tmp_append(w: Walk, b: u8) -> Walk {
    let mut n = w;
    if (n.tmp_len as usize) < TMP_CAP {
        n.tmp[n.tmp_len as usize] = b;
        n.tmp_len += 1;
    } else {
        n.tmp_over = true;
    }
    n
}

/// The innermost open container is an object on the path (level 2 is the array).
pub fn spec_walk_in_obj(w: &Walk) -> bool {
    w.depth == w.on && (w.on == 1 || w.on >= 3)
}

/// A scalar value just ended: store it where its key says, and clear the
/// scratch. Nothing happens unless a scalar was being captured.
pub fn spec_walk_finish_scalar(w: Walk) -> Walk {
    if w.cap != Cap::Scalar || w.tmp_len as usize > TMP_CAP {
        return w;
    }
    let mut n = w;
    let s = &w.tmp[..w.tmp_len as usize];
    match w.key {
        Key::Ok => n.ok_true = !w.tmp_over && s == b"true",
        Key::UpdateId => n.cur.update_id = if w.tmp_over { None } else { spec_parse_u64(s) },
        Key::Date => n.cur.date = if w.tmp_over { None } else { spec_parse_u64(s) },
        Key::Id => n.cur.chat_id = if w.tmp_over { None } else { spec_parse_i64(s) },
        _ => {}
    }
    tmp_reset(n)
}

/// `{` (`is_obj`) or `[`: one level deeper; follows the path
/// root{ "result":[ update{ "message":{ "chat":{} | "from":{} } } ] } when this
/// container is the next one on it. Deeper than MAX_DEPTH is an error.
pub fn spec_walk_open(w: Walk, is_obj: bool) -> Walk {
    let mut n = w;
    if w.depth >= MAX_DEPTH {
        n.err = true;
        return n;
    }
    let on_path = w.depth == w.on;
    let val = !w.expect_key;
    let enters = on_path
        && match w.on {
            0 => is_obj,
            1 => !is_obj && val && w.key == Key::Result,
            2 => is_obj,
            3 => is_obj && val && w.key == Key::Message,
            4 => is_obj && val && (w.key == Key::Chat || w.key == Key::From),
            _ => false,
        };
    n.depth = w.depth + 1;
    if enters {
        n.on = w.on + 1;
        if w.on == 2 {
            n.cur = RAW_EMPTY;
        }
        if w.on == 4 {
            n.in_from = w.key == Key::From;
        }
        n.expect_key = is_obj;
    }
    n.key = Key::None;
    tmp_reset(n)
}

/// `}` or `]`: one level up. On the path: a scalar in progress is stored, and
/// closing an update object (level 3) hands out the collected update. Closing
/// with nothing open is an error; closing the root marks the body done.
pub fn spec_walk_close(w: Walk) -> (Walk, Option<RawUpdate>) {
    let mut n = w;
    if w.depth == 0 {
        n.err = true;
        return (n, None);
    }
    let mut out = None;
    if w.depth == w.on {
        if spec_walk_in_obj(&w) {
            n = spec_walk_finish_scalar(n);
        }
        if w.on == 3 {
            out = Some(n.cur);
            n.cur = RAW_EMPTY;
        }
        n.on = w.on - 1;
        n.expect_key = false;
        n.key = Key::None;
    }
    n.depth = w.depth - 1;
    if n.depth == 0 {
        n.done = true;
    }
    (tmp_reset(n), out)
}

/// `"` opening a string inside an on-path object: decide what to capture
/// (a key, the message text, the username, or nothing).
pub fn spec_walk_str_start(w: Walk) -> Walk {
    if !spec_walk_in_obj(&w) {
        return w;
    }
    let mut n = tmp_reset(w);
    n.cap = if w.expect_key {
        Cap::Key
    } else if w.on == 4 && w.key == Key::Text {
        Cap::Text
    } else if w.on == 5 && w.in_from && w.key == Key::Username {
        Cap::User
    } else {
        Cap::None
    };
    n
}

/// `"` closing a string inside an on-path object: a captured key is
/// classified, a captured text or username is stored in the current update.
pub fn spec_walk_str_end(w: Walk) -> Walk {
    if !spec_walk_in_obj(&w) || w.tmp_len as usize > TMP_CAP {
        return w;
    }
    let mut n = w;
    let s = &w.tmp[..w.tmp_len as usize];
    match w.cap {
        Cap::Key => n.key = if w.tmp_over { Key::Other } else { spec_classify_key(w.on, w.in_from, s) },
        Cap::Text => {
            n.cur.has_text = true;
            n.cur.text_over = w.tmp_over;
            n.cur.text = [0; CMD_TEXT_CAP];
            n.cur.text_len = 0;
            if !w.tmp_over {
                let mut i = 0;
                while i < s.len() {
                    n.cur.text[i] = s[i];
                    i += 1;
                }
                n.cur.text_len = w.tmp_len;
            }
        }
        Cap::User => n.cur.who = spec_name_from(s, w.tmp_over),
        _ => {}
    }
    tmp_reset(n)
}

/// One byte of a number / true / false / null: captured when it is the value
/// of `ok`, `update_id`, `date` or `id` in an on-path object.
pub fn spec_walk_scalar(w: Walk, b: u8) -> Walk {
    let mut n = w;
    if spec_walk_in_obj(&w) && !w.expect_key && matches!(w.key, Key::Ok | Key::UpdateId | Key::Date | Key::Id) {
        if w.cap != Cap::Scalar {
            n = tmp_reset(n);
            n.cap = Cap::Scalar;
        }
        n = spec_walk_tmp_append(n, b);
    }
    n
}

/// Structure tracker: consumes tokens, hands out one RawUpdate each time an
/// element object of the root's "result" array closes.
pub fn spec_walk_step(w: Walk, t: Tok) -> (Walk, Option<RawUpdate>) {
    if w.err {
        return (w, None);
    }
    let mut n = w;
    let skip = matches!(t, Tok::Skip);
    if !spec_walk_ok(&w) || matches!(t, Tok::Bad) || (w.done && !skip) {
        n.err = true;
        return (n, None);
    }
    let in_obj = spec_walk_in_obj(&w);
    match t {
        Tok::Skip | Tok::Bad => (n, None),
        Tok::ObjOpen => (spec_walk_open(w, true), None),
        Tok::ArrOpen => (spec_walk_open(w, false), None),
        Tok::ObjClose | Tok::ArrClose => spec_walk_close(w),
        Tok::Colon => {
            if in_obj {
                n.expect_key = false;
            }
            (n, None)
        }
        Tok::Comma => {
            if in_obj {
                n = spec_walk_finish_scalar(n);
                n.key = Key::None;
                n.expect_key = true;
            }
            (n, None)
        }
        Tok::StrStart => (spec_walk_str_start(w), None),
        Tok::StrByte(b) => {
            if in_obj && matches!(w.cap, Cap::Key | Cap::Text | Cap::User) {
                n = spec_walk_tmp_append(n, b);
            }
            (n, None)
        }
        Tok::StrEnd => (spec_walk_str_end(w), None),
        Tok::Scalar(b) => (spec_walk_scalar(w, b), None),
    }
}

pub fn spec_scan_byte(sc: Scan, b: u8) -> (Scan, Option<RawUpdate>) {
    let (lx, t) = spec_lex_step(sc.lx, b);
    let (w, out) = spec_walk_step(sc.w, t);
    (Scan { lx, w }, out)
}

/// The whole body was one complete JSON value whose root object has `"ok":true`.
pub fn spec_scan_finish(sc: Scan) -> bool {
    !sc.w.err && sc.w.done && sc.w.depth == 0 && sc.w.ok_true && sc.lx.mode == LexMode::Value
}

pub fn spec_to_update_in(r: RawUpdate, chat_id: i64) -> Option<UpdateIn> {
    match r.update_id {
        None => None,
        Some(id) => Some(UpdateIn {
            update_id: id,
            authorized: r.has_text && r.chat_id == Some(chat_id),
            cmd: if r.has_text && !r.text_over && r.text_len as usize <= CMD_TEXT_CAP {
                spec_parse_command(&r.text[..r.text_len as usize])
            } else {
                Cmd::Unknown
            },
            date: r.date,
            who: r.who,
        }),
    }
}

// ================================================================ inbox
pub fn spec_inbox_ok(q: &Inbox) -> bool {
    (q.head as usize) < INBOX_CAP && q.len as usize <= INBOX_CAP
}

/// Append; a full (or malformed) inbox is returned unchanged.
pub fn spec_inbox_push(q: Inbox, u: RawUpdate) -> Inbox {
    if !spec_inbox_ok(&q) || q.len as usize == INBOX_CAP {
        return q;
    }
    let mut n = q;
    n.items[(q.head as usize + q.len as usize) % INBOX_CAP] = u;
    n.len = q.len + 1;
    n
}

/// Remove the oldest; an empty (or malformed) inbox gives None and is returned unchanged.
pub fn spec_inbox_pop(q: Inbox) -> (Inbox, Option<RawUpdate>) {
    if !spec_inbox_ok(&q) || q.len == 0 {
        return (q, None);
    }
    let mut n = q;
    n.head = ((q.head as usize + 1) % INBOX_CAP) as u8;
    n.len = q.len - 1;
    (n, Some(q.items[q.head as usize]))
}

// ================================================================ over-the-air update
/// `a` is a strictly later release than `b`: compare major, then minor, then patch.
pub fn spec_version_newer(a: Version, b: Version) -> bool {
    (a.major, a.minor, a.patch) > (b.major, b.minor, b.patch)
}

/// One version part: 1 to 5 ASCII digits, value at most 65535.
pub fn spec_version_part(x: &[u8]) -> Option<u16> {
    if x.len() > 5 {
        return None;
    }
    match spec_parse_u64(x) {
        Some(v) if v <= 65_535 => Some(v as u16),
        _ => None,
    }
}

/// `[v]MAJOR.MINOR.PATCH[+build]`: at most VERSION_CAP bytes in all; one optional
/// leading `v`; everything from the first `+` on is build metadata and ignored; what is
/// left is exactly three parts split by two dots. Anything else (a `-rc1`, a fourth
/// part, an empty part) -> None.
pub fn spec_parse_version(s: &[u8]) -> Option<Version> {
    if s.len() > VERSION_CAP {
        return None;
    }
    let s = if !s.is_empty() && s[0] == b'v' { &s[1..] } else { s };
    let mut e = 0;
    while e < s.len() && s[e] != b'+' {
        e += 1;
    }
    let core = &s[..e];
    let mut dots = [0usize; 2];
    let mut n = 0;
    let mut i = 0;
    while i < core.len() {
        if core[i] == b'.' {
            if n == 2 {
                return None;
            }
            dots[n] = i;
            n += 1;
        }
        i += 1;
    }
    if n != 2 {
        return None;
    }
    match (
        spec_version_part(&core[..dots[0]]),
        spec_version_part(&core[dots[0] + 1..dots[1]]),
        spec_version_part(&core[dots[1] + 1..]),
    ) {
        (Some(major), Some(minor), Some(patch)) => Some(Version { major, minor, patch }),
        _ => None,
    }
}

/// Download the image the manifest names? Only a strictly newer version (never the
/// running one again, never an older one), whose size is non-zero and fits the spare
/// slot, and which is not the version that was just rolled back (`bad`).
pub fn spec_ota_plan(running: Version, m: Manifest, slot_size: u32, bad: Option<Version>) -> bool {
    let newer = spec_version_newer(m.version, running);
    newer && m.size > 0 && m.size <= slot_size && bad != Some(m.version)
}

/// Make the downloaded image the boot image? Only when the bytes are exactly what the
/// manifest promised (same size, same SHA-256), esp_ota_end accepted the image (its
/// format, hash and signature), the version inside the image is the manifest's, and
/// that version is still strictly newer than the running one and fits the slot.
pub fn spec_ota_accept(running: Version, m: Manifest, slot_size: u32, d: Download) -> bool {
    let newer = spec_version_newer(m.version, running);
    newer
        && m.size > 0
        && m.size <= slot_size
        && d.image_ok
        && d.size == m.size
        && d.sha256 == m.sha256
        && d.image_version == Some(m.version)
}

/// Confirm the running image (cancel the bootloader's rollback)? Only when it is still
/// pending verification and has completed a successful Telegram poll since boot.
pub fn spec_ota_mark_valid(pending_verify: bool, last_good_poll: Option<u64>) -> bool {
    pending_verify && last_good_poll.is_some()
}

// ================================================================ glue
/// One loop pass. First the update timer: at boot and every OTA_CHECK_INTERVAL_MS it
/// raises `update_pending`, exactly as a fresh /update would; the core's pick_op then
/// runs the check only when nothing is left to process or send. Then one core step.
pub fn spec_bot_step(b: Bot, i: StepIn) -> BotStep {
    let mut core = b.core;
    let mut last = b.ota_last_check;
    if spec_due(b.ota_last_check, i.now_ms, OTA_CHECK_INTERVAL_MS) {
        core.update_pending = true;
        last = Some(i.now_ms);
    }
    let o = spec_step(core, i);
    let mut n = b;
    n.ota_last_check = last;
    n.core = o.state;
    n.stats.loops = b.stats.loops.saturating_add(1);
    if let Op::Poll { .. } = o.op {
        n.scan = SCAN_START;
        n.inbox = INBOX_EMPTY;
    }
    BotStep { bot: n, op: o.op, heartbeat: o.heartbeat }
}

/// One byte of a getUpdates response body.
pub fn spec_bot_feed(b: Bot, byte: u8) -> Bot {
    if b.core.batch_left > 0 {
        return b;
    }
    let (scan, out) = spec_scan_byte(b.scan, byte);
    let mut n = b;
    n.scan = scan;
    if let Some(u) = out {
        n.inbox = spec_inbox_push(b.inbox, u);
    }
    n
}

/// The poll's HTTP exchange is over. `http_ok`: status 200 and the body was read to its end.
pub fn spec_bot_poll_done(b: Bot, http_ok: bool, date_hdr: &[u8], t_req_ms: u64, now: u64) -> Bot {
    if b.core.batch_left > 0 {
        return b;
    }
    let ok = http_ok && spec_scan_finish(b.scan) && spec_inbox_ok(&b.inbox);
    let mut n = b;
    if !ok {
        n.inbox = INBOX_EMPTY;
    }
    n.scan = SCAN_START;
    let p = PollResult { ok, count: n.inbox.len as u32, date_unix: spec_http_date(date_hdr), t_req_ms };
    n.core = spec_on_poll(b.core, p, now);
    n
}

/// Take the next update out of the inbox and act on it. true = press the relay now.
pub fn spec_bot_next_update(b: Bot, chat_id: i64, now: u64, door_open: bool) -> (Bot, bool) {
    if b.core.batch_left == 0 || b.core.pend_reply.is_some() {
        return (b, false);
    }
    let (inbox, item) = spec_inbox_pop(b.inbox);
    let mut n = b;
    n.inbox = inbox;
    match item {
        None => {
            n.core.batch_left = 0;
            (n, false)
        }
        Some(raw) => match spec_to_update_in(raw, chat_id) {
            None => {
                n.core.batch_left = b.core.batch_left - 1;
                (n, false)
            }
            Some(u) => {
                let (core, press) = spec_on_update(b.core, u, now, door_open);
                n.core = core;
                (n, press)
            }
        },
    }
}

/// The shell reports the result of sending `m` (the message of the Op::Send it
/// just performed). ok: the failure count goes back to 0. Failed: `m` goes back
/// into the slot it came from, unless that slot is taken or this was failure
/// number SEND_TRIES in a row; then `m` is dropped and the count restarts.
/// Nothing else in the state changes.
pub fn spec_send_done(s: State, m: Msg, ok: bool) -> State {
    let mut n = s;
    if ok || s.send_fails >= SEND_TRIES - 1 {
        n.send_fails = 0;
        return n;
    }
    n.send_fails = s.send_fails + 1;
    match m {
        Msg::Online => n.pend_online = true,
        Msg::Reply(o) => {
            if s.pend_reply.is_none() {
                n.pend_reply = Some(o);
            }
        }
        Msg::CloseTimeout { who } => {
            if s.pend_close_timeout.is_none() {
                n.pend_close_timeout = Some(who);
            }
        }
        Msg::OpenAlert { minutes } => {
            if s.pend_open_alert.is_none() {
                n.pend_open_alert = Some(minutes);
            }
        }
        Msg::ClosedAfter(c) => {
            if s.pend_closed_notice.is_none() {
                n.pend_closed_notice = Some(c);
            }
        }
    }
    n
}

pub fn spec_bot_send_done(b: Bot, m: Msg, ok: bool) -> Bot {
    let mut n = b;
    n.core = spec_send_done(b.core, m, ok);
    n
}

pub fn spec_bot_http_result(b: Bot, ok: bool, sock_err: bool) -> Bot {
    let mut n = b;
    if ok {
        n.stats.http_ok = b.stats.http_ok.saturating_add(1);
    } else {
        n.stats.http_fail = b.stats.http_fail.saturating_add(1);
    }
    if sock_err {
        n.stats.sock_err = b.stats.sock_err.saturating_add(1);
    }
    n
}
// Stand-ins for arithmetic the SAT solver cannot compare (see CONTRACTS.md,
// "Arithmetic the solver cannot do"). A harness swaps the real spec function
// for one of these with `#[kani::stub]`. Each stand-in returns ANY value that
// has the listed facts, and the same value when asked again for a remembered
// argument. A proof that holds for every such function holds for the real one,
// because the lemma harnesses below prove the real one has the facts.
#[cfg(kani)]
#[allow(static_mut_refs)]
pub mod opaque {
    const POW10: [u64; 20] = [
        1, 10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000, 1_000_000_000,
        10_000_000_000, 100_000_000_000, 1_000_000_000_000, 10_000_000_000_000, 100_000_000_000_000,
        1_000_000_000_000_000, 10_000_000_000_000_000, 100_000_000_000_000_000,
        1_000_000_000_000_000_000, 10_000_000_000_000_000_000,
    ];

    /// What a proof may know about `(q, r) = spec_div10(x)`: the digit is below
    /// 10, the quotient is at most an eighth, and a number below 10^k has a
    /// quotient below 10^(k-1).
    pub fn div10_facts(x: u64, q: u64, r: u8) -> bool {
        let mut ok = r < 10 && q <= x >> 3;
        let mut k = 1;
        while k < 20 {
            if x < POW10[k] && q >= POW10[k - 1] {
                ok = false;
            }
            k += 1;
        }
        ok
    }

    pub const CHAIN: usize = 21;
    static mut CH_X: [u64; CHAIN] = [0; CHAIN];
    static mut CH_Q: [u64; CHAIN] = [0; CHAIN];
    static mut CH_R: [u8; CHAIN] = [0; CHAIN];

    /// Call once at the start of a harness: fixes the stand-in on `v`, on its
    /// quotient, on that quotient, and so on (21 values).
    pub fn div10_chain(v: u64) {
        let mut x = v;
        let mut k = 0;
        while k < CHAIN {
            let q: u64 = kani::any();
            let r: u8 = kani::any();
            kani::assume(div10_facts(x, q, r));
            unsafe {
                CH_X[k] = x;
                CH_Q[k] = q;
                CH_R[k] = r;
            }
            x = q;
            k += 1;
        }
    }

    /// Stand-in for `spec_div10`. Off the chain it remembers nothing, which
    /// only makes the proof harder, never wrong.
    pub fn div10_opaque(x: u64) -> (u64, u8) {
        let mut k = 0;
        while k < CHAIN {
            if unsafe { CH_X[k] } == x {
                return unsafe { (CH_Q[k], CH_R[k]) };
            }
            k += 1;
        }
        let q: u64 = kani::any();
        let r: u8 = kani::any();
        kani::assume(div10_facts(x, q, r));
        (q, r)
    }

    // ---------------------------------------------------------------------
    // Record and replay. The proof of a function with callees replaces every
    // callee AND the callee's specification by the same stand-in
    // (`#[kani::stub]`). A stand-in knows nothing about the real answer: it
    // returns ANY value (with the one stated fact, if there is one). While
    // recording is on it remembers what it answered for which arguments;
    // afterwards it only repeats those answers. The harness evaluates the
    // specification of the function once with recording on, then runs the
    // function and compares. Any value includes the real one, so a proof that
    // passes with stand-ins also holds for the real callees, given that each
    // callee equals its own specification (its own proof). Arguments that
    // were not recorded get a fresh unknown answer, which can only make a
    // proof fail.
    //
    // `begin` (generated below, after the stand-ins) empties all tables and
    // turns recording on; `end` turns it off.
    static mut REC: bool = false;
    pub fn end() {
        unsafe { REC = false; }
    }

    /// A byte slice as a table key: its length and a zero-padded copy, packed
    /// 16 bytes to a word (W words). A slice longer than the key equals nothing
    /// (not even itself), so its answer is never replayed.
    #[derive(Clone, Copy)]
    pub struct SK<const W: usize> {
        n: usize,
        w: [u128; W],
    }
    #[no_mangle]
    pub fn gb_sk_fill(w: &mut [u128], s: &[u8]) {
        let mut i = 0;
        while i < s.len() && i < w.len() * 16 {
            w[i / 16] |= (s[i] as u128) << (8 * (i % 16));
            i += 1;
        }
    }
    #[no_mangle]
    pub fn gb_words_eq(a: &[u128], b: &[u128]) -> bool {
        let mut i = 0;
        let mut ok = a.len() == b.len();
        while i < a.len() && i < b.len() {
            if a[i] != b[i] {
                ok = false;
            }
            i += 1;
        }
        ok
    }
    impl<const W: usize> PartialEq for SK<W> {
        fn eq(&self, o: &Self) -> bool {
            self.n <= 16 * W && self.n == o.n && gb_words_eq(&self.w, &o.w)
        }
    }
    pub fn sk<const W: usize>(s: &[u8]) -> SK<W> {
        let mut w = [0u128; W];
        gb_sk_fill(&mut w, s);
        SK { n: s.len(), w }
    }
    /// Equality of two Texts, 16 bytes at a time (the same answer as `==`,
    /// proven by `lemma_text_eq`; much cheaper for the solver than 512 single bytes).
    #[no_mangle]
    pub fn gb_text_eq(a: &crate::Text, b: &crate::Text) -> bool {
        if a.len != b.len || a.overflow != b.overflow {
            return false;
        }
        let x: [u128; crate::TEXT_CAP / 16] = unsafe { core::mem::transmute(a.buf) };
        let y: [u128; crate::TEXT_CAP / 16] = unsafe { core::mem::transmute(b.buf) };
        let mut i = 0;
        let mut ok = true;
        while i < crate::TEXT_CAP / 16 {
            if x[i] != y[i] {
                ok = false;
            }
            i += 1;
        }
        ok
    }

    pub fn keq<K, F: Fn(&K, &K) -> bool>(f: F, a: &K, b: &K) -> bool {
        f(a, b)
    }
    pub fn holds<R, F: Fn(&R) -> bool>(f: F, v: &R) -> bool {
        f(v)
    }

    // standin!(name, slots, in order?, (params) -> return type, key type, key value, key equality, fact)
    //
    // While recording, a stand-in answers with any value for which the fact
    // holds (the fact may mention the arguments) and stores arguments and
    // answer in the next free slot. Afterwards:
    //   in order = false: the answer is the first stored answer whose
    //     arguments are equal, else any value with the fact. While recording it
    //     also looks up earlier calls, so equal arguments get equal answers.
    //   in order = true (stand-ins with a Text argument, where one comparison
    //     is costly): the j-th call after recording gets the j-th stored answer
    //     if its arguments equal the j-th stored arguments, else any value with
    //     the fact. A function checked with such a stand-in must make these
    //     calls in the order of its specification.
    // Either way an answer is replayed only for arguments equal to the ones it
    // was first given for, and is otherwise unconstrained beyond the fact.
    macro_rules! standin {
        ($name:ident, $slots:expr, $seq:literal, ($($a:ident : $t:ty),*) -> $r:ty, $kt:ty, $key:expr, $eq:expr, $fact:expr) => {
            pub mod $name {
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_imports)]
                use crate::*;
                const NK: Option<$kt> = None;
                const NV: Option<$r> = None;
                pub static mut N: usize = 0;
                pub static mut J: usize = 0;
                static mut K: [Option<$kt>; $slots] = [NK; $slots];
                static mut V: [Option<$r>; $slots] = [NV; $slots];
                #[allow(unused_comparisons)]
                fn known(key: &$kt) -> Option<$r> {
                    unsafe {
                        if $seq {
                            if REC {
                                return None;
                            }
                            let j = J;
                            J += 1;
                            standin!(@at ($eq), key, j, $slots, 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15);
                        } else {
                            standin!(@look ($eq), key, $slots, 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15);
                        }
                    }
                    None
                }
                #[allow(unused_variables)]
                pub fn f($($a: $t),*) -> $r {
                    let key: $kt = $key;
                    let v: $r = match known(&key) {
                        Some(v) => v,
                        None => {
                            let v: $r = kani::any();
                            kani::assume(holds($fact, &v));
                            v
                        }
                    };
                    unsafe {
                        if REC && N < $slots {
                            K[N] = Some(key);
                            V[N] = Some(v);
                            N += 1;
                        }
                    }
                    v
                }
            }
        };
        (@look $eq:tt, $key:ident, $slots:expr, $($j:literal)*) => {
            $(
                if $j < $slots && $j < N {
                    if let (Some(k), Some(v)) = (&K[$j], V[$j]) {
                        if keq($eq, k, $key) {
                            return Some(v);
                        }
                    }
                }
            )*
        };
        (@at $eq:tt, $key:ident, $jv:ident, $slots:expr, $($j:literal)*) => {
            $(
                if $j < $slots && $jv == $j && $j < N {
                    if let (Some(k), Some(v)) = (&K[$j], V[$j]) {
                        if keq($eq, k, $key) {
                            return Some(v);
                        }
                    }
                }
            )*
        };
    }

    standin!(o_uptime_s, 4, false, (now_ms: u64) -> u64, (u64,), (now_ms,), |a, b| a.0 == b.0, |_| true);
    standin!(o_due, 4, false, (last: Option<u64>, now: u64, interval: u64) -> bool, (Option<u64>, u64, u64,), (last, now, interval,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_whole_minutes, 4, false, (elapsed_ms: u64) -> u64, (u64,), (elapsed_ms,), |a, b| a.0 == b.0, |_| true);
    standin!(o_tenths, 4, false, (elapsed_ms: u64) -> u64, (u64,), (elapsed_ms,), |a, b| a.0 == b.0, |_| true);
    standin!(o_anchor, 4, false, (date: u64, t_req_ms: u64, now: u64) -> Option<Anchor>, (u64, u64, u64,), (date, t_req_ms, now,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_boot_unix, 4, false, (k: Anchor) -> u64, (Anchor,), (k,), |a, b| a.0 == b.0, |_| true);
    standin!(o_age, 4, false, (anchor: Option<Anchor>, boot_unix: Option<u64>, now: u64, date: Option<u64>) -> Age, (Option<Anchor>, Option<u64>, u64, Option<u64>,), (anchor, boot_unix, now, date,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && a.3 == b.3, |_| true);
    standin!(o_decide, 4, false, (cmd: Cmd, age: Age, door_open: bool) -> Decision, (Cmd, Age, bool,), (cmd, age, door_open,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_on_update, 4, false, (s: State, u: UpdateIn, now: u64, door_open: bool) -> (State, bool), (State, UpdateIn, u64, bool,), (s, u, now, door_open,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && a.3 == b.3, |_| true);
    standin!(o_on_poll, 4, false, (s: State, p: PollResult, now: u64) -> State, (State, PollResult, u64,), (s, p, now,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_closed_notice, 4, false, (d: DoorState, now: u64) -> Option<ClosedNotice>, (DoorState, u64,), (d, now,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_open_alert_due, 4, false, (d: DoorState, now: u64) -> bool, (DoorState, u64,), (d, now,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_close_timeout_due, 4, false, (c: CloseReq, now: u64) -> bool, (CloseReq, u64,), (c, now,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_door, 4, false, (d: DoorState, now: u64, open: bool) -> (DoorState, DoorEvents), (DoorState, u64, bool,), (d, now, open,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_deaf, 4, false, (last_good_poll: Option<u64>, now: u64) -> bool, (Option<u64>, u64,), (last_good_poll, now,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_pick_op, 4, false, (n: State, now: u64, overrun: bool, wifi_up: bool) -> Op, (State, u64, bool, bool,), (n, now, overrun, wifi_up,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && a.3 == b.3, |_| true);
    standin!(o_overrun, 4, false, (last_step: Option<u64>, now: u64) -> bool, (Option<u64>, u64,), (last_step, now,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_send_done, 4, false, (s: State, m: Msg, ok: bool) -> State, (State, Msg, bool,), (s, m, ok,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_queue_door_events, 4, false, (s: State, ev: DoorEvents, door_open: bool) -> State, (State, DoorEvents, bool,), (s, ev, door_open,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_wifi_track, 4, false, (since: Option<u64>, now: u64, wifi_up: bool) -> Option<u64>, (Option<u64>, u64, bool,), (since, now, wifi_up,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_op_done, 4, false, (s: State, op: Op, now: u64) -> State, (State, Op, u64,), (s, op, now,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_step, 4, false, (s: State, i: StepIn) -> StepOut, (State, StepIn,), (s, i,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_text_push, 12, true, (t: Text, s: &[u8]) -> Text, (SK<6>, Text,), (sk(s), t,), |a, b| a.0 == b.0 && gb_text_eq(&a.1, &b.1), |_| true);
    standin!(o_div10, 4, false, (v: u64) -> (u64, u8), (u64,), (v,), |a, b| a.0 == b.0, |_| true);
    standin!(o_dec, 4, false, (v: u64) -> ([u8; 20], usize), (u64,), (v,), |a, b| a.0 == b.0, |v: &([u8; 20], usize)| v.1 < 20);
    standin!(o_div60, 4, false, (v: u64) -> (u64, u64), (u64,), (v,), |a, b| a.0 == b.0, |_| true);
    standin!(o_text_push_u64, 8, true, (t: Text, v: u64) -> Text, (u64, Text,), (v, t,), |a, b| a.0 == b.0 && gb_text_eq(&a.1, &b.1), |_| true);
    standin!(o_text_push_i64, 4, true, (t: Text, v: i64) -> Text, (i64, Text,), (v, t,), |a, b| a.0 == b.0 && gb_text_eq(&a.1, &b.1), |_| true);
    standin!(o_text_push_padded, 4, true, (t: Text, v: u64, width: u8) -> Text, (u64, u8, Text,), (v, width, t,), |a, b| a.0 == b.0 && a.1 == b.1 && gb_text_eq(&a.2, &b.2), |_| true);
    standin!(o_json_escape, 8, false, (c: u8) -> ([u8; 6], u8), (u8,), (c,), |a, b| a.0 == b.0, |v: &([u8; 6], u8)| v.1 <= 6);
    standin!(o_text_push_json, 4, true, (t: Text, s: &[u8]) -> Text, (SK<32>, Text,), (sk(s), t,), |a, b| a.0 == b.0 && gb_text_eq(&a.1, &b.1), |_| true);
    standin!(o_push_mention, 4, true, (t: Text, who: &Name) -> Text, (Name, Text,), (*who, t,), |a, b| a.0 == b.0 && gb_text_eq(&a.1, &b.1), |_| true);
    standin!(o_render_closed_after, 4, false, (c: ClosedNotice) -> Text, (ClosedNotice,), (c,), |a, b| a.0 == b.0, |_| true);
    standin!(o_render_diag, 4, false, (version: &[u8], d: Diag) -> Text, (SK<6>, Diag,), (sk(version), d,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_render_reply, 4, false, (o: Outgoing, version: &[u8], d: Diag) -> Text, (Outgoing, SK<6>, Diag,), (o, sk(version), d,), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_parse_command, 4, false, (t: &[u8]) -> Cmd, (SK<6>,), (sk(t),), |a, b| a.0 == b.0, |_| true);
    standin!(o_parse_u64, 4, false, (s: &[u8]) -> Option<u64>, (SK<6>,), (sk(s),), |a, b| a.0 == b.0, |v: &Option<u64>| s.len() > 18 || match v { Some(x) => *x < 1_000_000_000_000_000_000, None => true });
    standin!(o_parse_i64, 4, false, (s: &[u8]) -> Option<i64>, (SK<6>,), (sk(s),), |a, b| a.0 == b.0, |_| true);
    standin!(o_http_date, 4, false, (s: &[u8]) -> Option<u64>, (SK<6>,), (sk(s),), |a, b| a.0 == b.0, |_| true);
    standin!(o_lex_step, 4, false, (lx: Lex, b: u8) -> (Lex, Tok), (Lex, u8,), (lx, b,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_classify_key, 4, false, (on: u8, in_from: bool, k: &[u8]) -> Key, (u8, bool, SK<6>,), (on, in_from, sk(k),), |a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2, |_| true);
    standin!(o_name_from, 4, false, (s: &[u8], over: bool) -> Name, (SK<6>, bool,), (sk(s), over,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_walk_tmp_append, 4, false, (w: Walk, b: u8) -> Walk, (Walk, u8,), (w, b,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_walk_finish_scalar, 4, false, (w: Walk) -> Walk, (Walk,), (w,), |a, b| a.0 == b.0, |_| true);
    standin!(o_walk_open, 4, false, (w: Walk, is_obj: bool) -> Walk, (Walk, bool,), (w, is_obj,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_walk_close, 4, false, (w: Walk) -> (Walk, Option<RawUpdate>), (Walk,), (w,), |a, b| a.0 == b.0, |_| true);
    standin!(o_walk_str_start, 4, false, (w: Walk) -> Walk, (Walk,), (w,), |a, b| a.0 == b.0, |_| true);
    standin!(o_walk_str_end, 4, false, (w: Walk) -> Walk, (Walk,), (w,), |a, b| a.0 == b.0, |_| true);
    standin!(o_walk_scalar, 4, false, (w: Walk, b: u8) -> Walk, (Walk, u8,), (w, b,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_walk_step, 4, false, (w: Walk, t: Tok) -> (Walk, Option<RawUpdate>), (Walk, Tok,), (w, t,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_scan_byte, 4, false, (sc: Scan, b: u8) -> (Scan, Option<RawUpdate>), (Scan, u8,), (sc, b,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_scan_finish, 4, false, (sc: Scan) -> bool, (Scan,), (sc,), |a, b| a.0 == b.0, |_| true);
    standin!(o_to_update_in, 4, false, (r: RawUpdate, chat_id: i64) -> Option<UpdateIn>, (RawUpdate, i64,), (r, chat_id,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_inbox_push, 4, false, (q: Inbox, u: RawUpdate) -> Inbox, (Inbox, RawUpdate,), (q, u,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    standin!(o_inbox_pop, 4, false, (q: Inbox) -> (Inbox, Option<RawUpdate>), (Inbox,), (q,), |a, b| a.0 == b.0, |_| true);
    standin!(o_version_newer, 4, false, (a: Version, b: Version) -> bool, (Version, Version,), (a, b,), |a, b| a.0 == b.0 && a.1 == b.1, |_| true);
    pub mod m_text_push_pad6 {
        #[allow(unused_imports)]
        use super::*;
        #[allow(unused_imports)]
        use crate::*;
        pub fn w_text_push_pad6(t: Text, v: u64) -> Text {
            o_text_push_padded::f(t, v, 6)
        }
    }
    pub use m_text_push_pad6::w_text_push_pad6;
    /// Empty every stand-in table and start recording.
    pub fn begin() {
        unsafe {
            o_uptime_s::N = 0;
            o_uptime_s::J = 0;
            o_due::N = 0;
            o_due::J = 0;
            o_whole_minutes::N = 0;
            o_whole_minutes::J = 0;
            o_tenths::N = 0;
            o_tenths::J = 0;
            o_anchor::N = 0;
            o_anchor::J = 0;
            o_boot_unix::N = 0;
            o_boot_unix::J = 0;
            o_age::N = 0;
            o_age::J = 0;
            o_decide::N = 0;
            o_decide::J = 0;
            o_on_update::N = 0;
            o_on_update::J = 0;
            o_on_poll::N = 0;
            o_on_poll::J = 0;
            o_closed_notice::N = 0;
            o_closed_notice::J = 0;
            o_open_alert_due::N = 0;
            o_open_alert_due::J = 0;
            o_close_timeout_due::N = 0;
            o_close_timeout_due::J = 0;
            o_door::N = 0;
            o_door::J = 0;
            o_deaf::N = 0;
            o_deaf::J = 0;
            o_pick_op::N = 0;
            o_pick_op::J = 0;
            o_overrun::N = 0;
            o_overrun::J = 0;
            o_send_done::N = 0;
            o_send_done::J = 0;
            o_queue_door_events::N = 0;
            o_queue_door_events::J = 0;
            o_wifi_track::N = 0;
            o_wifi_track::J = 0;
            o_op_done::N = 0;
            o_op_done::J = 0;
            o_step::N = 0;
            o_step::J = 0;
            o_text_push::N = 0;
            o_text_push::J = 0;
            o_div10::N = 0;
            o_div10::J = 0;
            o_dec::N = 0;
            o_dec::J = 0;
            o_div60::N = 0;
            o_div60::J = 0;
            o_text_push_u64::N = 0;
            o_text_push_u64::J = 0;
            o_text_push_i64::N = 0;
            o_text_push_i64::J = 0;
            o_text_push_padded::N = 0;
            o_text_push_padded::J = 0;
            o_json_escape::N = 0;
            o_json_escape::J = 0;
            o_text_push_json::N = 0;
            o_text_push_json::J = 0;
            o_push_mention::N = 0;
            o_push_mention::J = 0;
            o_render_closed_after::N = 0;
            o_render_closed_after::J = 0;
            o_render_diag::N = 0;
            o_render_diag::J = 0;
            o_render_reply::N = 0;
            o_render_reply::J = 0;
            o_parse_command::N = 0;
            o_parse_command::J = 0;
            o_parse_u64::N = 0;
            o_parse_u64::J = 0;
            o_parse_i64::N = 0;
            o_parse_i64::J = 0;
            o_http_date::N = 0;
            o_http_date::J = 0;
            o_lex_step::N = 0;
            o_lex_step::J = 0;
            o_classify_key::N = 0;
            o_classify_key::J = 0;
            o_name_from::N = 0;
            o_name_from::J = 0;
            o_walk_tmp_append::N = 0;
            o_walk_tmp_append::J = 0;
            o_walk_finish_scalar::N = 0;
            o_walk_finish_scalar::J = 0;
            o_walk_open::N = 0;
            o_walk_open::J = 0;
            o_walk_close::N = 0;
            o_walk_close::J = 0;
            o_walk_str_start::N = 0;
            o_walk_str_start::J = 0;
            o_walk_str_end::N = 0;
            o_walk_str_end::J = 0;
            o_walk_scalar::N = 0;
            o_walk_scalar::J = 0;
            o_walk_step::N = 0;
            o_walk_step::J = 0;
            o_scan_byte::N = 0;
            o_scan_byte::J = 0;
            o_scan_finish::N = 0;
            o_scan_finish::J = 0;
            o_to_update_in::N = 0;
            o_to_update_in::J = 0;
            o_inbox_push::N = 0;
            o_inbox_push::J = 0;
            o_inbox_pop::N = 0;
            o_inbox_pop::J = 0;
            o_version_newer::N = 0;
            o_version_newer::J = 0;
            REC = true;
        }
    }
}
// @end

// @fn door_tick
/// Door monitoring for one pass: open alerts, close timeout, closed notice.
/// Contract: the result equals `spec_door(d, now, open)`. No panic for any input.
pub fn door_tick(d: DoorState, now: u64, open: bool) -> (DoorState, DoorEvents) {
    // @body
    let mut ev = DoorEvents { open_alert: None, close_timeout: None, closed_notice: None };
    if !open {
        ev.closed_notice = closed_notice(d, now);
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
            if open_alert_due(d, now) {
                ev.open_alert = Some(whole_minutes(now.saturating_sub(s0)));
                n.last_alert = Some(now);
            }
        }
    }
    if let Some(c) = d.close {
        if close_timeout_due(c, now) {
            ev.close_timeout = Some(c.who);
            n.close = Some(CloseReq { at: c.at, who: c.who, alerted: true });
        }
    }
    (n, ev)
}
// @end

// @harness contract_door_tick target=door_tick stubs=closed_notice,open_alert_due,close_timeout_due,whole_minutes uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(closed_notice, opaque::o_closed_notice::f)]
#[kani::stub(spec_closed_notice, opaque::o_closed_notice::f)]
#[kani::stub(open_alert_due, opaque::o_open_alert_due::f)]
#[kani::stub(spec_open_alert_due, opaque::o_open_alert_due::f)]
#[kani::stub(close_timeout_due, opaque::o_close_timeout_due::f)]
#[kani::stub(spec_close_timeout_due, opaque::o_close_timeout_due::f)]
#[kani::stub(whole_minutes, opaque::o_whole_minutes::f)]
#[kani::stub(spec_whole_minutes, opaque::o_whole_minutes::f)]
fn contract_door_tick() {
    let d: DoorState = kani::any();
    let now: u64 = kani::any();
    let open: bool = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_door(d, now, open);
    opaque::end();
    let got = door_tick(d, now, open);
    assert!(got == want);
}
// @end

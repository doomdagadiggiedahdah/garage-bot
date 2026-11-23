// @fn pick_op
/// The priority chain: which single op this pass performs.
/// Contract: the result equals `spec_pick_op(n, now, overrun, wifi_up)`. No panic for any input.
pub fn pick_op(n: State, now: u64, overrun: bool, wifi_up: bool) -> Op {
    // @body
    if overrun {
        return Op::Reboot;
    }
    if deaf(n.last_good_poll, now) {
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
    if interval_due(n.last_poll, now, POLL_INTERVAL_MS) {
        return Op::Poll { offset: n.next_offset };
    }
    Op::Idle
}
// @end

// @harness contract_pick_op target=pick_op stubs=deaf,interval_due uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(deaf, opaque::o_deaf::f)]
#[kani::stub(spec_deaf, opaque::o_deaf::f)]
#[kani::stub(interval_due, opaque::o_due::f)]
#[kani::stub(spec_due, opaque::o_due::f)]
fn contract_pick_op() {
    let n: State = kani::any();
    let now: u64 = kani::any();
    let overrun: bool = kani::any();
    let wifi_up: bool = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_pick_op(n, now, overrun, wifi_up);
    opaque::end();
    let got = pick_op(n, now, overrun, wifi_up);
    assert!(got == want);
}
// @end

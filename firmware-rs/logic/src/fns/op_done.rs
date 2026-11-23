// @fn op_done
/// Clear the slot the chosen op serves, or stamp the poll time, or clear the update flag.
/// Contract: the result equals `spec_op_done(s, op, now)`. No panic for any input.
pub fn op_done(s: State, op: Op, now: u64) -> State {
    // @body
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
// @end

// @harness contract_op_done target=op_done stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_op_done() {
    let s: State = kani::any();
    let op: Op = kani::any();
    let now: u64 = kani::any();
    let want = spec_op_done(s, op, now);
    let got = op_done(s, op, now);
    assert!(got == want);
}
// @end

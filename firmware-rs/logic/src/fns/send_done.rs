// @fn send_done
/// The shell reports how the send of `m` went: after a failure `m` goes back into its slot, unless SEND_TRIES sends in a row have failed.
/// Contract: the result equals `spec_send_done(s, m, ok)`. No panic for any input.
pub fn send_done(s: State, m: Msg, ok: bool) -> State {
    // @body
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
// @end

// @harness contract_send_done target=send_done stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_send_done() {
    let s: State = kani::any();
    let m: Msg = kani::any();
    let ok: bool = kani::any();
    let want = spec_send_done(s, m, ok);
    let got = send_done(s, m, ok);
    assert!(got == want);
}
// @end

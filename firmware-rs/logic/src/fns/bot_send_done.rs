// @fn bot_send_done
/// The shell calls this after every Op::Send, with the message and whether Telegram accepted it.
/// Contract: the result equals `spec_bot_send_done(b, m, ok)`. No panic for any input.
pub fn bot_send_done(b: Bot, m: Msg, ok: bool) -> Bot {
    // @body
    let mut n = b;
    n.core = send_done(b.core, m, ok);
    n
}
// @end

// @harness contract_bot_send_done target=bot_send_done stubs=send_done uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(send_done, opaque::o_send_done::f)]
#[kani::stub(spec_send_done, opaque::o_send_done::f)]
fn contract_bot_send_done() {
    let b: Bot = kani::any();
    let m: Msg = kani::any();
    let ok: bool = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_bot_send_done(b, m, ok);
    opaque::end();
    let got = bot_send_done(b, m, ok);
    assert!(got == want);
}
// @end

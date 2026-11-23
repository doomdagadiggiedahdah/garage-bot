// @fn bot_feed
/// One byte of a getUpdates response body.
/// Contract: the result equals `spec_bot_feed(b, byte)`. No panic for any input.
pub fn bot_feed(b: Bot, byte: u8) -> Bot {
    // @body
    if b.core.batch_left > 0 {
        return b;
    }
    let (scan, out) = scan_byte(b.scan, byte);
    let mut n = b;
    n.scan = scan;
    if let Some(u) = out {
        n.inbox = inbox_push(b.inbox, u);
    }
    n
}
// @end

// @harness contract_bot_feed target=bot_feed stubs=scan_byte,inbox_push uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(scan_byte, opaque::o_scan_byte::f)]
#[kani::stub(spec_scan_byte, opaque::o_scan_byte::f)]
#[kani::stub(inbox_push, opaque::o_inbox_push::f)]
#[kani::stub(spec_inbox_push, opaque::o_inbox_push::f)]
fn contract_bot_feed() {
    let b: Bot = kani::any();
    let byte: u8 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_bot_feed(b, byte);
    opaque::end();
    let got = bot_feed(b, byte);
    assert!(got == want);
}
// @end

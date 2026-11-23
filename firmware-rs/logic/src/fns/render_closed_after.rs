// @fn render_closed_after
/// `@name Door closed after X.Y minutes.`
/// Contract: the result equals `spec_render_closed_after(c)`. No panic for any input.
pub fn render_closed_after(c: ClosedNotice) -> Text {
    // @body
    let p = text_push;
    let (q, r) = div10(c.tenths);
    let t = push_mention(Text::EMPTY, &c.who);
    let t = p(t, b"Door closed after ");
    let t = text_push_u64(t, q);
    let t = p(t, b".");
    let t = text_push_u64(t, r as u64);
    p(t, b" minutes.")
}
// @end

// @harness contract_render_closed_after target=render_closed_after stubs=div10,push_mention,text_push,text_push_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(div10, opaque::o_div10::f)]
#[kani::stub(spec_div10, opaque::o_div10::f)]
#[kani::stub(push_mention, opaque::o_push_mention::f)]
#[kani::stub(spec_push_mention, opaque::o_push_mention::f)]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
fn contract_render_closed_after() {
    let c: ClosedNotice = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_closed_after(c);
    opaque::end();
    let got = render_closed_after(c);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

// @fn render_poll_path
/// URL path and query for getUpdates.
/// Contract: the result equals `spec_render_poll_path(offset)`. No panic for any input.
pub fn render_poll_path(offset: u64) -> Text {
    // @body
    let t = text_push(Text::EMPTY, b"getUpdates?offset=");
    let t = text_push_u64(t, offset);
    text_push(t, b"&timeout=2&limit=8")
}
// @end

// @harness contract_render_poll_path target=render_poll_path stubs=text_push,text_push_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
fn contract_render_poll_path() {
    let offset: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_poll_path(offset);
    opaque::end();
    let got = render_poll_path(offset);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

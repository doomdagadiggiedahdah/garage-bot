// @fn render_heap
/// The HEAP telemetry line.
/// Contract: the result equals `spec_render_heap(d)`. No panic for any input.
pub fn render_heap(d: Diag) -> Text {
    // @body
    let p = text_push;
    let t = p(Text::EMPTY, b"HEAP ");
    let t = text_push_u64(t, div60(d.uptime_s).0);
    let t = p(t, b",");
    let t = text_push_u64(t, d.free_mem as u64);
    let t = p(t, b",");
    let t = text_push_u64(t, d.stats.loops);
    let t = p(t, b",");
    let t = text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b",");
    let t = text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b",");
    text_push_u64(t, d.stats.sock_err as u64)
}
// @end

// @harness contract_render_heap target=render_heap stubs=text_push,div60,text_push_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(div60, opaque::o_div60::f)]
#[kani::stub(spec_div60, opaque::o_div60::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
fn contract_render_heap() {
    let d: Diag = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_heap(d);
    opaque::end();
    let got = render_heap(d);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

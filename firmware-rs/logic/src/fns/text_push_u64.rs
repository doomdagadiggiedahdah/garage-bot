// @fn text_push_u64
/// Append a decimal number.
/// Contract: the result equals `spec_text_push_u64(t, v)`. No panic for any input.
pub fn text_push_u64(t: Text, v: u64) -> Text {
    // @body
    let (b, i) = dec(v);
    text_push(t, &b[i..])
}
// @end

// @harness contract_text_push_u64 target=text_push_u64 stubs=dec,text_push uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(dec, opaque::o_dec::f)]
#[kani::stub(spec_dec, opaque::o_dec::f)]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
fn contract_text_push_u64() {
    let t: Text = kani::any();
    let v: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_text_push_u64(t, v);
    opaque::end();
    let got = text_push_u64(t, v);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

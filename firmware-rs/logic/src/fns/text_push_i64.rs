// @fn text_push_i64
/// Append a signed decimal number.
/// Contract: the result equals `spec_text_push_i64(t, v)`. No panic for any input.
pub fn text_push_i64(t: Text, v: i64) -> Text {
    // @body
    if v < 0 {
        text_push_u64(text_push(t, b"-"), v.unsigned_abs())
    } else {
        text_push_u64(t, v as u64)
    }
}
// @end

// @harness contract_text_push_i64 target=text_push_i64 stubs=text_push,text_push_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
fn contract_text_push_i64() {
    let t: Text = kani::any();
    let v: i64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_text_push_i64(t, v);
    opaque::end();
    let got = text_push_i64(t, v);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

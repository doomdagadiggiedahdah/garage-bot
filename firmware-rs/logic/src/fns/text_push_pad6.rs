// @fn text_push_pad6
/// Append a decimal number right-aligned with spaces in 6 columns (never truncated).
/// Contract: the result equals `spec_text_push_padded(t, v, 6)`. No panic for any input.
pub fn text_push_pad6(t: Text, v: u64) -> Text {
    // @body
    let width: u8 = 6;
    let (b, i) = dec(v);
    let digits = 20 - i;
    let mut r = t;
    let mut k = digits;
    while k < width as usize {
        r = text_push(r, b" ");
        k += 1;
    }
    text_push(r, &b[i..])
}
// @end

// @harness contract_text_push_pad6 target=text_push_pad6 stubs=dec,text_push uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(8)]
#[kani::stub(dec, opaque::o_dec::f)]
#[kani::stub(spec_dec, opaque::o_dec::f)]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
fn contract_text_push_pad6() {
    let t: Text = kani::any();
    let v: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_text_push_padded(t, v, 6);
    opaque::end();
    let got = text_push_pad6(t, v);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

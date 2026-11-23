// @fn text_push_json
/// Append bytes escaped for the inside of a JSON string.
/// Contract: the result equals `spec_text_push_json(t, s)`. No panic for any input.
pub fn text_push_json(t: Text, s: &[u8]) -> Text {
    // @body
    let mut r = t;
    let mut i = 0;
    while i < s.len() {
        let (e, n) = json_escape(s[i]);
        r = text_push(r, &e[..n as usize]);
        i += 1;
    }
    r
}
// @end

// @harness contract_text_push_json target=text_push_json stubs=json_escape,text_push uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(10)]
#[kani::stub(json_escape, opaque::o_json_escape::f)]
#[kani::stub(spec_json_escape, opaque::o_json_escape::f)]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
fn contract_text_push_json() {
    let t: Text = kani::any();
    let s_buf: [u8; 8] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 8);
    let s: &[u8] = &s_buf[..s_len];
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_text_push_json(t, s);
    opaque::end();
    let got = text_push_json(t, s);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

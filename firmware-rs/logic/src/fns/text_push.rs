// @fn text_push
/// Append bytes; all or nothing; overflow is sticky.
/// Contract: the result equals `spec_text_push(t, s)`. No panic for any input.
pub fn text_push(t: Text, s: &[u8]) -> Text {
    // @body
    if t.overflow || t.len as usize > TEXT_CAP || s.len() > TEXT_CAP - t.len as usize {
        return Text { overflow: true, ..t };
    }
    let mut r = t;
    let mut i = 0;
    while i < s.len() {
        r.buf[t.len as usize + i] = s[i];
        i += 1;
    }
    r.len = t.len + s.len() as u16;
    r
}
// @end

// @harness contract_text_push target=text_push stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(26)]
fn contract_text_push() {
    let t: Text = kani::any();
    let s_buf: [u8; 24] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 24);
    let s: &[u8] = &s_buf[..s_len];
    let want = spec_text_push(t, s);
    let got = text_push(t, s);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

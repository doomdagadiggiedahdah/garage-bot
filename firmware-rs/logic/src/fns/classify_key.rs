// @fn classify_key
/// Which known JSON key the bytes spell at a path level.
/// Contract: the result equals `spec_classify_key(on, in_from, k)`. No panic for any input.
pub fn classify_key(on: u8, in_from: bool, k: &[u8]) -> Key {
    // @body
    match (on, in_from, k) {
        (1, _, b"ok") => Key::Ok,
        (1, _, b"result") => Key::Result,
        (3, _, b"update_id") => Key::UpdateId,
        (3, _, b"message") => Key::Message,
        (4, _, b"date") => Key::Date,
        (4, _, b"text") => Key::Text,
        (4, _, b"chat") => Key::Chat,
        (4, _, b"from") => Key::From,
        (5, false, b"id") => Key::Id,
        (5, true, b"username") => Key::Username,
        _ => Key::Other,
    }
}
// @end

// @harness contract_classify_key target=classify_key stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(12)]
fn contract_classify_key() {
    let on: u8 = kani::any();
    let in_from: bool = kani::any();
    let k_buf: [u8; 10] = kani::any();
    let k_len: usize = kani::any();
    kani::assume(k_len <= 10);
    let k: &[u8] = &k_buf[..k_len];
    let want = spec_classify_key(on, in_from, k);
    let got = classify_key(on, in_from, k);
    assert!(got == want);
}
// @end

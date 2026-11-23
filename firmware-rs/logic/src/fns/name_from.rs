// @fn name_from
/// Validate captured bytes as a username.
/// Contract: the result equals `spec_name_from(s, over)`. No panic for any input.
pub fn name_from(s: &[u8], over: bool) -> Name {
    // @body
    let mut n = Name::EMPTY;
    if over || s.is_empty() || s.len() > NAME_CAP {
        return Name::EMPTY;
    }
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if !(c.is_ascii_alphanumeric() || c == b'_') {
            return Name::EMPTY;
        }
        n.bytes[i] = c;
        i += 1;
    }
    n.len = s.len() as u8;
    n
}
// @end

// @harness contract_name_from target=name_from stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(36)]
fn contract_name_from() {
    let s_buf: [u8; 34] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 34);
    let s: &[u8] = &s_buf[..s_len];
    let over: bool = kani::any();
    let want = spec_name_from(s, over);
    let got = name_from(s, over);
    assert!(got == want);
}
// @end

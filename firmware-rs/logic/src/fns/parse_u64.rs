// @fn parse_u64
/// 1 to 19 ASCII digits.
/// Contract: the result equals `spec_parse_u64(s)`. No panic for any input.
pub fn parse_u64(s: &[u8]) -> Option<u64> {
    // @body
    if s.is_empty() || s.len() > 19 {
        return None;
    }
    let mut v: u64 = 0;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        v = v * 10 + (c - b'0') as u64;
        i += 1;
    }
    Some(v)
}
// @end

// @harness contract_parse_u64 target=parse_u64 stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(23)]
fn contract_parse_u64() {
    let s_buf: [u8; 21] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 21);
    let s: &[u8] = &s_buf[..s_len];
    let want = spec_parse_u64(s);
    let got = parse_u64(s);
    assert!(got == want);
}
// @end

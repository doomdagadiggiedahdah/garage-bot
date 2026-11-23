// @fn parse_version
/// `[v]MAJOR.MINOR.PATCH[+build]`, each part 1 to 5 digits and at most 65535.
/// Contract: the result equals `spec_parse_version(s)`. No panic for any input.
pub fn parse_version(s: &[u8]) -> Option<Version> {
    // @body
    if s.len() > VERSION_CAP {
        return None;
    }
    // One pass: digits accumulate into the current part, a dot moves to the next part,
    // a '+' (or the end) finishes. Any other byte, an empty part, a sixth digit or a
    // third dot rejects.
    let mut i = if !s.is_empty() && s[0] == b'v' { 1 } else { 0 };
    let mut parts = [0u32; 3];
    let mut k = 0usize;
    let mut digits = 0usize;
    while i < s.len() && s[i] != b'+' {
        let c = s[i];
        if c == b'.' {
            if digits == 0 || k == 2 {
                return None;
            }
            k += 1;
            digits = 0;
        } else if c >= b'0' && c <= b'9' {
            if digits == 5 {
                return None;
            }
            parts[k] = parts[k] * 10 + (c - b'0') as u32;
            digits += 1;
        } else {
            return None;
        }
        i += 1;
    }
    if k != 2 || digits == 0 || parts[0] > 65_535 || parts[1] > 65_535 || parts[2] > 65_535 {
        return None;
    }
    Some(Version { major: parts[0] as u16, minor: parts[1] as u16, patch: parts[2] as u16 })
}
// @end

// @harness contract_parse_version target=parse_version stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(28)]
fn contract_parse_version() {
    let s_buf: [u8; 26] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 26);
    let s: &[u8] = &s_buf[..s_len];
    let want = spec_parse_version(s);
    let got = parse_version(s);
    assert!(got == want);
}
// @end

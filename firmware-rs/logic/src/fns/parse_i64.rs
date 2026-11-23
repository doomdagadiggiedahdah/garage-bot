// @fn parse_i64
/// Optional minus sign, then 1 to 18 ASCII digits.
/// Contract: the result equals `spec_parse_i64(s)`. No panic for any input.
pub fn parse_i64(s: &[u8]) -> Option<i64> {
    // @body
    let neg = !s.is_empty() && s[0] == b'-';
    let d = if neg { &s[1..] } else { s };
    if d.len() > 18 {
        return None;
    }
    let v = parse_u64(d)?;
    Some(if neg { -(v as i64) } else { v as i64 })
}
// @end

// @harness contract_parse_i64 target=parse_i64 stubs=parse_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(23)]
#[kani::stub(parse_u64, opaque::o_parse_u64::f)]
#[kani::stub(spec_parse_u64, opaque::o_parse_u64::f)]
fn contract_parse_i64() {
    let s_buf: [u8; 21] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 21);
    let s: &[u8] = &s_buf[..s_len];
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_parse_i64(s);
    opaque::end();
    let got = parse_i64(s);
    assert!(got == want);
}
// @end

// @fn div60
/// Quotient and remainder by 60: `(v / 60, v % 60)`.
/// Contract: the result equals `spec_div60(v)`. No panic for any input.
pub fn div60(v: u64) -> (u64, u64) {
    // @body
    (v / 60, v % 60)
}
// @end

// @harness contract_div60 target=div60 stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_div60() {
    let v: u64 = kani::any();
    let want = spec_div60(v);
    let got = div60(v);
    assert!(got == want);
}
// @end

// @fn div10
/// One decimal step: `(v / 10, v % 10)`.
/// Contract: the result equals `spec_div10(v)`. No panic for any input.
pub fn div10(v: u64) -> (u64, u8) {
    // @body
    (v / 10, (v % 10) as u8)
}
// @end

// @harness contract_div10 target=div10 stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_div10() {
    let v: u64 = kani::any();
    let want = spec_div10(v);
    let got = div10(v);
    assert!(got == want);
}
// @end

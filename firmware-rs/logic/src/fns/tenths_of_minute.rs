// @fn tenths_of_minute
/// Minutes in tenths, rounded half up, saturating.
/// Contract: the result equals `spec_tenths(elapsed_ms)`. No panic for any input.
pub fn tenths_of_minute(elapsed_ms: u64) -> u64 {
    // @body
    elapsed_ms.saturating_add(3_000) / 1000 / 6
}
// @end

// @harness contract_tenths_of_minute target=tenths_of_minute stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_tenths_of_minute() {
    let elapsed_ms: u64 = kani::any();
    let want = spec_tenths(elapsed_ms);
    let got = tenths_of_minute(elapsed_ms);
    assert!(got == want);
}
// @end

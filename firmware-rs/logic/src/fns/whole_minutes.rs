// @fn whole_minutes
/// Whole minutes in a duration.
/// Contract: the result equals `spec_whole_minutes(elapsed_ms)`. No panic for any input.
pub fn whole_minutes(elapsed_ms: u64) -> u64 {
    // @body
    elapsed_ms / 1000 / 60
}
// @end

// @harness contract_whole_minutes target=whole_minutes stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_whole_minutes() {
    let elapsed_ms: u64 = kani::any();
    let want = spec_whole_minutes(elapsed_ms);
    let got = whole_minutes(elapsed_ms);
    assert!(got == want);
}
// @end

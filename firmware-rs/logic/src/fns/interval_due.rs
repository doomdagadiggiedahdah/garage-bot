// @fn interval_due
/// True when `interval` ms have passed since `last` (None = never ran = due). Saturating: a `last` in the future is not due unless interval is 0.
/// Contract: the result equals `spec_due(last, now, interval)`. No panic for any input.
pub fn interval_due(last: Option<u64>, now: u64, interval: u64) -> bool {
    // @body
    match last {
        None => true,
        Some(t) => now.saturating_sub(t) >= interval,
    }
}
// @end

// @harness contract_interval_due target=interval_due stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_interval_due() {
    let last: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let interval: u64 = kani::any();
    let want = spec_due(last, now, interval);
    let got = interval_due(last, now, interval);
    assert!(got == want);
}
// @end

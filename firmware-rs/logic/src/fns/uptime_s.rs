// @fn uptime_s
/// Seconds since boot from the monotonic clock. Never accumulated, so it cannot drift.
/// Contract: the result equals `spec_uptime_s(now_ms)`. No panic for any input.
pub fn uptime_s(now_ms: u64) -> u64 {
    // @body
    now_ms / 1000
}
// @end

// @harness contract_uptime_s target=uptime_s stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_uptime_s() {
    let now_ms: u64 = kani::any();
    let want = spec_uptime_s(now_ms);
    let got = uptime_s(now_ms);
    assert!(got == want);
}
// @end

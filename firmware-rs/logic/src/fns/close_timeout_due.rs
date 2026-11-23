// @fn close_timeout_due
/// Has a pending bot close timed out without being reported yet?
/// Contract: the result equals `spec_close_timeout_due(c, now)`. No panic for any input.
pub fn close_timeout_due(c: CloseReq, now: u64) -> bool {
    // @body
    !c.alerted && now.saturating_sub(c.at) >= CLOSE_TIMEOUT_MS
}
// @end

// @harness contract_close_timeout_due target=close_timeout_due stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_close_timeout_due() {
    let c: CloseReq = kani::any();
    let now: u64 = kani::any();
    let want = spec_close_timeout_due(c, now);
    let got = close_timeout_due(c, now);
    assert!(got == want);
}
// @end

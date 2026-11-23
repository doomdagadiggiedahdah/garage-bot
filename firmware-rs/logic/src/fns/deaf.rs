// @fn deaf
/// No successful poll for DEAF_REBOOT_AFTER_MS (counted from boot when there was none yet).
/// Contract: the result equals `spec_deaf(last_good_poll, now)`. No panic for any input.
pub fn deaf(last_good_poll: Option<u64>, now: u64) -> bool {
    // @body
    let since = match last_good_poll {
        Some(t) => t,
        None => 0,
    };
    now.saturating_sub(since) >= DEAF_REBOOT_AFTER_MS
}
// @end

// @harness contract_deaf target=deaf stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_deaf() {
    let last_good_poll: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let want = spec_deaf(last_good_poll, now);
    let got = deaf(last_good_poll, now);
    assert!(got == want);
}
// @end

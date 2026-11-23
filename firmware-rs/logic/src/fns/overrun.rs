// @fn overrun
/// Was the previous pass more than the watchdog timeout ago, or did the clock go back?
/// Contract: the result equals `spec_overrun(last_step, now)`. No panic for any input.
pub fn overrun(last_step: Option<u64>, now: u64) -> bool {
    // @body
    match last_step {
        // a clock that went back is treated like a missed watchdog: reboot
        Some(t) => now < t || now - t > WDT_TIMEOUT_MS,
        None => false,
    }
}
// @end

// @harness contract_overrun target=overrun stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_overrun() {
    let last_step: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let want = spec_overrun(last_step, now);
    let got = overrun(last_step, now);
    assert!(got == want);
}
// @end

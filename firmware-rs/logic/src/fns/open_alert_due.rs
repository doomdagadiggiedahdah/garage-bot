// @fn open_alert_due
/// Is an open-door alert due on this pass?
/// Contract: the result equals `spec_open_alert_due(d, now)`. No panic for any input.
pub fn open_alert_due(d: DoorState, now: u64) -> bool {
    // @body
    match d.open_since {
        None => false,
        Some(s0) => {
            let due = match d.last_alert {
                None => now.saturating_sub(s0) >= INITIAL_ALERT_MS,
                Some(a) => now.saturating_sub(a) >= REPEAT_ALERT_MS,
            };
            due && !d.muted
        }
    }
}
// @end

// @harness contract_open_alert_due target=open_alert_due stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_open_alert_due() {
    let d: DoorState = kani::any();
    let now: u64 = kani::any();
    let want = spec_open_alert_due(d, now);
    let got = open_alert_due(d, now);
    assert!(got == want);
}
// @end

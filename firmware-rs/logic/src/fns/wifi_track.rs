// @fn wifi_track
/// Since when WiFi has been down (None while it is up).
/// Contract: the result equals `spec_wifi_track(since, now, wifi_up)`. No panic for any input.
pub fn wifi_track(since: Option<u64>, now: u64, wifi_up: bool) -> Option<u64> {
    // @body
    if wifi_up {
        None
    } else {
        match since {
            Some(t) => Some(t),
            None => Some(now),
        }
    }
}
// @end

// @harness contract_wifi_track target=wifi_track stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_wifi_track() {
    let since: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let wifi_up: bool = kani::any();
    let want = spec_wifi_track(since, now, wifi_up);
    let got = wifi_track(since, now, wifi_up);
    assert!(got == want);
}
// @end

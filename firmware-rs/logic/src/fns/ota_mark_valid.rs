// @fn ota_mark_valid
/// Confirm a freshly installed image? Only after it completed a successful Telegram poll.
/// Contract: the result equals `spec_ota_mark_valid(pending_verify, last_good_poll)`. No panic for any input.
pub fn ota_mark_valid(pending_verify: bool, last_good_poll: Option<u64>) -> bool {
    // @body
    match last_good_poll {
        Some(_) => pending_verify,
        None => false,
    }
}
// @end

// @harness contract_ota_mark_valid target=ota_mark_valid stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_ota_mark_valid() {
    let pending_verify: bool = kani::any();
    let last_good_poll: Option<u64> = kani::any();
    let want = spec_ota_mark_valid(pending_verify, last_good_poll);
    let got = ota_mark_valid(pending_verify, last_good_poll);
    assert!(got == want);
}
// @end

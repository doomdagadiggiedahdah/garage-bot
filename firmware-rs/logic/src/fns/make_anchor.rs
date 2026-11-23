// @fn make_anchor
/// Build a time anchor from an HTTP Date header value, or None when the inputs are implausible.
/// Contract: the result equals `spec_anchor(date, t_req_ms, now)`. No panic for any input.
pub fn make_anchor(date: u64, t_req_ms: u64, now: u64) -> Option<Anchor> {
    // @body
    if date < MAX_UNIX_S && t_req_ms <= now && date + 1 >= t_req_ms / 1000 {
        Some(Anchor { unix_s: date + 1, mono_ms: t_req_ms })
    } else {
        None
    }
}
// @end

// @harness contract_make_anchor target=make_anchor stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_make_anchor() {
    let date: u64 = kani::any();
    let t_req_ms: u64 = kani::any();
    let now: u64 = kani::any();
    let want = spec_anchor(date, t_req_ms, now);
    let got = make_anchor(date, t_req_ms, now);
    assert!(got == want);
}
// @end

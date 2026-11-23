// @fn closed_notice
/// The 'door closed after X.Y minutes' notice, when a bot close was pending and the door was open long enough.
/// Contract: the result equals `spec_closed_notice(d, now)`. No panic for any input.
pub fn closed_notice(d: DoorState, now: u64) -> Option<ClosedNotice> {
    // @body
    if let (Some(s0), Some(c)) = (d.open_since, d.close) {
        let e = now.saturating_sub(s0);
        if e >= CLOSED_NOTICE_MIN_MS {
            return Some(ClosedNotice { who: c.who, tenths: tenths_of_minute(e) });
        }
    }
    None
}
// @end

// @harness contract_closed_notice target=closed_notice stubs=tenths_of_minute uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(tenths_of_minute, opaque::o_tenths::f)]
#[kani::stub(spec_tenths, opaque::o_tenths::f)]
fn contract_closed_notice() {
    let d: DoorState = kani::any();
    let now: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_closed_notice(d, now);
    opaque::end();
    let got = closed_notice(d, now);
    assert!(got == want);
}
// @end

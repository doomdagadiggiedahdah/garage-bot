// @fn on_poll
/// Account for a finished getUpdates call.
/// Contract: the result equals `spec_on_poll(s, p, now)`. No panic for any input.
pub fn on_poll(s: State, p: PollResult, now: u64) -> State {
    // @body
    if s.batch_left > 0 {
        return s;
    }
    let mut n = s;
    if !p.ok {
        n.quiesced = false;
        return n;
    }
    n.last_good_poll = Some(now);
    n.batch_left = if p.count > MAX_BATCH { MAX_BATCH } else { p.count };
    n.quiesced = p.count == 0;
    if let Some(d) = p.date_unix {
        if let Some(k) = make_anchor(d, p.t_req_ms, now) {
            n.anchor = Some(k);
            if s.boot_unix.is_none() {
                n.boot_unix = Some(boot_bound(k));
            }
        }
    }
    n
}
// @end

// @harness contract_on_poll target=on_poll stubs=make_anchor,boot_bound uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(make_anchor, opaque::o_anchor::f)]
#[kani::stub(spec_anchor, opaque::o_anchor::f)]
#[kani::stub(boot_bound, opaque::o_boot_unix::f)]
#[kani::stub(spec_boot_unix, opaque::o_boot_unix::f)]
fn contract_on_poll() {
    let s: State = kani::any();
    let p: PollResult = kani::any();
    let now: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_on_poll(s, p, now);
    opaque::end();
    let got = on_poll(s, p, now);
    assert!(got == want);
}
// @end

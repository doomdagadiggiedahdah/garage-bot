// @fn judge_age
/// How old is a message dated `date`? Errs on the side of older.
/// Contract: the result equals `spec_age(anchor, boot_unix, now, date)`. No panic for any input.
pub fn judge_age(anchor: Option<Anchor>, boot_unix: Option<u64>, now: u64, date: Option<u64>) -> Age {
    // @body
    let (k, b, d) = match (anchor, boot_unix, date) {
        (Some(k), Some(b), Some(d)) => (k, b, d),
        _ => return Age::Unknown,
    };
    let usable = k.unix_s <= MAX_UNIX_S
        && now >= k.mono_ms
        && now - k.mono_ms <= ANCHOR_MAX_AGE_MS
        && k.unix_s >= k.mono_ms / 1000;
    if !usable {
        return Age::Unknown;
    }
    let nu = k.unix_s + (now - k.mono_ms) / 1000 + NOW_SLACK_S;
    if d > nu {
        Age::Unknown
    } else if d < b {
        Age::PreBoot
    } else if nu - d > STALE_AFTER_S {
        Age::Stale(nu - d)
    } else {
        Age::Fresh(nu - d)
    }
}
// @end

// @harness contract_judge_age target=judge_age stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_judge_age() {
    let anchor: Option<Anchor> = kani::any();
    let boot_unix: Option<u64> = kani::any();
    let now: u64 = kani::any();
    let date: Option<u64> = kani::any();
    let want = spec_age(anchor, boot_unix, now, date);
    let got = judge_age(anchor, boot_unix, now, date);
    assert!(got == want);
}
// @end

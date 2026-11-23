// @fn boot_bound
/// Upper bound on the unix second of this boot, from an anchor.
/// Contract: the result equals `spec_boot_unix(k)`. No panic for any input.
pub fn boot_bound(k: Anchor) -> u64 {
    // @body
    k.unix_s.saturating_add(1).saturating_sub(k.mono_ms / 1000)
}
// @end

// @harness contract_boot_bound target=boot_bound stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_boot_bound() {
    let k: Anchor = kani::any();
    let want = spec_boot_unix(k);
    let got = boot_bound(k);
    assert!(got == want);
}
// @end

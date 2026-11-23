// @fn ota_plan
/// Download the manifest's image? Only a strictly newer version that fits the slot and is not the one that just rolled back.
/// Contract: the result equals `spec_ota_plan(running, m, slot_size, bad)`. No panic for any input.
pub fn ota_plan(running: Version, m: Manifest, slot_size: u32, bad: Option<Version>) -> bool {
    // @body
    let newer = version_newer(m.version, running);
    if !newer || m.size == 0 || m.size > slot_size {
        return false;
    }
    match bad {
        Some(v) => v != m.version,
        None => true,
    }
}
// @end

// @harness contract_ota_plan target=ota_plan stubs=version_newer uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(version_newer, opaque::o_version_newer::f)]
#[kani::stub(spec_version_newer, opaque::o_version_newer::f)]
fn contract_ota_plan() {
    let running: Version = kani::any();
    let m: Manifest = kani::any();
    let slot_size: u32 = kani::any();
    let bad: Option<Version> = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_ota_plan(running, m, slot_size, bad);
    opaque::end();
    let got = ota_plan(running, m, slot_size, bad);
    assert!(got == want);
}
// @end

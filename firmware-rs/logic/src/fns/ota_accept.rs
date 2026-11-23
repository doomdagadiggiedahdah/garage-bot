// @fn ota_accept
/// Boot the downloaded image? Only if size, SHA-256 and embedded version match the manifest and the install is still allowed.
/// Contract: the result equals `spec_ota_accept(running, m, slot_size, d)`. No panic for any input.
pub fn ota_accept(running: Version, m: Manifest, slot_size: u32, d: Download) -> bool {
    // @body
    let newer = version_newer(m.version, running);
    if !newer || m.size == 0 || m.size > slot_size || !d.image_ok || d.size != m.size {
        return false;
    }
    let mut i = 0;
    while i < 32 {
        if d.sha256[i] != m.sha256[i] {
            return false;
        }
        i += 1;
    }
    match d.image_version {
        Some(v) => v == m.version,
        None => false,
    }
}
// @end

// @harness contract_ota_accept target=ota_accept stubs=version_newer uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(34)]
#[kani::stub(version_newer, opaque::o_version_newer::f)]
#[kani::stub(spec_version_newer, opaque::o_version_newer::f)]
fn contract_ota_accept() {
    let running: Version = kani::any();
    let m: Manifest = kani::any();
    let slot_size: u32 = kani::any();
    let d: Download = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_ota_accept(running, m, slot_size, d);
    opaque::end();
    let got = ota_accept(running, m, slot_size, d);
    assert!(got == want);
}
// @end

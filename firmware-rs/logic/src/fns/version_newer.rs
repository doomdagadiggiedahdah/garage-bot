// @fn version_newer
/// `a` is a strictly later release than `b` (major, then minor, then patch).
/// Contract: the result equals `spec_version_newer(a, b)`. No panic for any input.
pub fn version_newer(a: Version, b: Version) -> bool {
    // @body
    if a.major != b.major {
        return a.major > b.major;
    }
    if a.minor != b.minor {
        return a.minor > b.minor;
    }
    a.patch > b.patch
}
// @end

// @harness contract_version_newer target=version_newer stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_version_newer() {
    let a: Version = kani::any();
    let b: Version = kani::any();
    let want = spec_version_newer(a, b);
    let got = version_newer(a, b);
    assert!(got == want);
}
// @end

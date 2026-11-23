// @fn scan_finish
/// Was the body one complete JSON value with ok:true?
/// Contract: the result equals `spec_scan_finish(sc)`. No panic for any input.
pub fn scan_finish(sc: Scan) -> bool {
    // @body
    !sc.w.err && sc.w.done && sc.w.depth == 0 && sc.w.ok_true && sc.lx.mode == LexMode::Value
}
// @end

// @harness contract_scan_finish target=scan_finish stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_scan_finish() {
    let sc: Scan = kani::any();
    let want = spec_scan_finish(sc);
    let got = scan_finish(sc);
    assert!(got == want);
}
// @end

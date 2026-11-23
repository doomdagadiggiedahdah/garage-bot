// @fn scan_byte
/// Tokenizer plus tracker, one byte.
/// Contract: the result equals `spec_scan_byte(sc, b)`. No panic for any input.
pub fn scan_byte(sc: Scan, b: u8) -> (Scan, Option<RawUpdate>) {
    // @body
    let (lx, t) = lex_step(sc.lx, b);
    let (w, out) = walk_step(sc.w, t);
    (Scan { lx, w }, out)
}
// @end

// @harness contract_scan_byte target=scan_byte stubs=lex_step,walk_step uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(lex_step, opaque::o_lex_step::f)]
#[kani::stub(spec_lex_step, opaque::o_lex_step::f)]
#[kani::stub(walk_step, opaque::o_walk_step::f)]
#[kani::stub(spec_walk_step, opaque::o_walk_step::f)]
fn contract_scan_byte() {
    let sc: Scan = kani::any();
    let b: u8 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_scan_byte(sc, b);
    opaque::end();
    let got = scan_byte(sc, b);
    assert!(got == want);
}
// @end

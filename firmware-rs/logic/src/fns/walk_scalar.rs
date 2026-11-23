// @fn walk_scalar
/// One byte of a number or literal: captured when its key is ok, update_id, date or id.
/// Contract: the result equals `spec_walk_scalar(w, b)`. No panic for any input.
pub fn walk_scalar(w: Walk, b: u8) -> Walk {
    // @body
    fn h_walk_in_obj(w: &Walk) -> bool {
        w.depth == w.on && (w.on == 1 || w.on >= 3)
    }
    fn h_tmp_reset(mut n: Walk) -> Walk {
        n.cap = Cap::None;
        n.tmp_len = 0;
        n.tmp_over = false;
        n
    }
    let mut n = w;
    if h_walk_in_obj(&w) && !w.expect_key && matches!(w.key, Key::Ok | Key::UpdateId | Key::Date | Key::Id) {
        if w.cap != Cap::Scalar {
            n = h_tmp_reset(n);
            n.cap = Cap::Scalar;
        }
        n = walk_tmp_append(n, b);
    }
    n
}
// @end

// @harness contract_walk_scalar target=walk_scalar stubs=walk_tmp_append uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(walk_tmp_append, opaque::o_walk_tmp_append::f)]
#[kani::stub(spec_walk_tmp_append, opaque::o_walk_tmp_append::f)]
fn contract_walk_scalar() {
    let w: Walk = kani::any();
    let b: u8 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_walk_scalar(w, b);
    opaque::end();
    let got = walk_scalar(w, b);
    assert!(got == want);
}
// @end

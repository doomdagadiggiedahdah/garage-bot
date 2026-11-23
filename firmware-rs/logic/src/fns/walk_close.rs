// @fn walk_close
/// `}` or `]`: one level up; closing an update object hands the update out.
/// Contract: the result equals `spec_walk_close(w)`. No panic for any input.
pub fn walk_close(w: Walk) -> (Walk, Option<RawUpdate>) {
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
    if w.depth == 0 {
        n.err = true;
        return (n, None);
    }
    let mut out = None;
    if w.depth == w.on {
        if h_walk_in_obj(&w) {
            n = walk_finish_scalar(n);
        }
        if w.on == 3 {
            out = Some(n.cur);
            n.cur = RAW_EMPTY;
        }
        n.on = w.on - 1;
        n.expect_key = false;
        n.key = Key::None;
    }
    n.depth = w.depth - 1;
    if n.depth == 0 {
        n.done = true;
    }
    (h_tmp_reset(n), out)
}
// @end

// @harness contract_walk_close target=walk_close stubs=walk_finish_scalar uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(walk_finish_scalar, opaque::o_walk_finish_scalar::f)]
#[kani::stub(spec_walk_finish_scalar, opaque::o_walk_finish_scalar::f)]
fn contract_walk_close() {
    let w: Walk = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_walk_close(w);
    opaque::end();
    let got = walk_close(w);
    assert!(got == want);
}
// @end

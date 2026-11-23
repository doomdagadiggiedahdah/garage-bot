// @fn walk_step
/// JSON structure tracker, one token.
/// Contract: the result equals `spec_walk_step(w, t)`. No panic for any input.
pub fn walk_step(w: Walk, t: Tok) -> (Walk, Option<RawUpdate>) {
    // @body
    fn h_walk_in_obj(w: &Walk) -> bool {
        w.depth == w.on && (w.on == 1 || w.on >= 3)
    }
    fn h_walk_ok(w: &Walk) -> bool {
        w.on <= w.depth && w.on <= 5 && w.depth <= MAX_DEPTH && w.tmp_len as usize <= TMP_CAP
    }
    if w.err {
        return (w, None);
    }
    let mut n = w;
    let skip = matches!(t, Tok::Skip);
    if !h_walk_ok(&w) || matches!(t, Tok::Bad) || (w.done && !skip) {
        n.err = true;
        return (n, None);
    }
    let in_obj = h_walk_in_obj(&w);
    match t {
        Tok::Skip | Tok::Bad => (n, None),
        Tok::ObjOpen => (walk_open(w, true), None),
        Tok::ArrOpen => (walk_open(w, false), None),
        Tok::ObjClose | Tok::ArrClose => walk_close(w),
        Tok::Colon => {
            if in_obj {
                n.expect_key = false;
            }
            (n, None)
        }
        Tok::Comma => {
            if in_obj {
                n = walk_finish_scalar(n);
                n.key = Key::None;
                n.expect_key = true;
            }
            (n, None)
        }
        Tok::StrStart => (walk_str_start(w), None),
        Tok::StrByte(b) => {
            if in_obj && matches!(w.cap, Cap::Key | Cap::Text | Cap::User) {
                n = walk_tmp_append(n, b);
            }
            (n, None)
        }
        Tok::StrEnd => (walk_str_end(w), None),
        Tok::Scalar(b) => (walk_scalar(w, b), None),
    }
}
// @end

// @harness contract_walk_step target=walk_step stubs=walk_open,walk_close,walk_finish_scalar,walk_str_start,walk_tmp_append,walk_str_end,walk_scalar uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(walk_open, opaque::o_walk_open::f)]
#[kani::stub(spec_walk_open, opaque::o_walk_open::f)]
#[kani::stub(walk_close, opaque::o_walk_close::f)]
#[kani::stub(spec_walk_close, opaque::o_walk_close::f)]
#[kani::stub(walk_finish_scalar, opaque::o_walk_finish_scalar::f)]
#[kani::stub(spec_walk_finish_scalar, opaque::o_walk_finish_scalar::f)]
#[kani::stub(walk_str_start, opaque::o_walk_str_start::f)]
#[kani::stub(spec_walk_str_start, opaque::o_walk_str_start::f)]
#[kani::stub(walk_tmp_append, opaque::o_walk_tmp_append::f)]
#[kani::stub(spec_walk_tmp_append, opaque::o_walk_tmp_append::f)]
#[kani::stub(walk_str_end, opaque::o_walk_str_end::f)]
#[kani::stub(spec_walk_str_end, opaque::o_walk_str_end::f)]
#[kani::stub(walk_scalar, opaque::o_walk_scalar::f)]
#[kani::stub(spec_walk_scalar, opaque::o_walk_scalar::f)]
fn contract_walk_step() {
    let w: Walk = kani::any();
    let t: Tok = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_walk_step(w, t);
    opaque::end();
    let got = walk_step(w, t);
    assert!(got == want);
}
// @end

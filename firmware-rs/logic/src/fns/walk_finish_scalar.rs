// @fn walk_finish_scalar
/// A scalar value ended: store it where its key says and clear the scratch.
/// Contract: the result equals `spec_walk_finish_scalar(w)`. No panic for any input.
pub fn walk_finish_scalar(w: Walk) -> Walk {
    // @body
    fn h_tmp_reset(mut n: Walk) -> Walk {
        n.cap = Cap::None;
        n.tmp_len = 0;
        n.tmp_over = false;
        n
    }
    if w.cap != Cap::Scalar || w.tmp_len as usize > TMP_CAP {
        return w;
    }
    let mut n = w;
    let s = &w.tmp[..w.tmp_len as usize];
    match w.key {
        Key::Ok => n.ok_true = !w.tmp_over && s == b"true",
        Key::UpdateId => n.cur.update_id = if w.tmp_over { None } else { parse_u64(s) },
        Key::Date => n.cur.date = if w.tmp_over { None } else { parse_u64(s) },
        Key::Id => n.cur.chat_id = if w.tmp_over { None } else { parse_i64(s) },
        _ => {}
    }
    h_tmp_reset(n)
}
// @end

// @harness contract_walk_finish_scalar target=walk_finish_scalar stubs=parse_u64,parse_i64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(parse_u64, opaque::o_parse_u64::f)]
#[kani::stub(spec_parse_u64, opaque::o_parse_u64::f)]
#[kani::stub(parse_i64, opaque::o_parse_i64::f)]
#[kani::stub(spec_parse_i64, opaque::o_parse_i64::f)]
fn contract_walk_finish_scalar() {
    let w: Walk = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_walk_finish_scalar(w);
    opaque::end();
    let got = walk_finish_scalar(w);
    assert!(got == want);
}
// @end

// @fn walk_tmp_append
/// Append one byte to the scratch buffer, or mark it overflowed when full.
/// Contract: the result equals `spec_walk_tmp_append(w, b)`. No panic for any input.
pub fn walk_tmp_append(w: Walk, b: u8) -> Walk {
    // @body
    let mut n = w;
    if (n.tmp_len as usize) < TMP_CAP {
        n.tmp[n.tmp_len as usize] = b;
        n.tmp_len += 1;
    } else {
        n.tmp_over = true;
    }
    n
}
// @end

// @harness contract_walk_tmp_append target=walk_tmp_append stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_walk_tmp_append() {
    let w: Walk = kani::any();
    let b: u8 = kani::any();
    let want = spec_walk_tmp_append(w, b);
    let got = walk_tmp_append(w, b);
    assert!(got == want);
}
// @end

// @fn walk_str_start
/// A string opens: decide what to capture (key, message text, username, nothing).
/// Contract: the result equals `spec_walk_str_start(w)`. No panic for any input.
pub fn walk_str_start(w: Walk) -> Walk {
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
    if !h_walk_in_obj(&w) {
        return w;
    }
    let mut n = h_tmp_reset(w);
    n.cap = if w.expect_key {
        Cap::Key
    } else if w.on == 4 && w.key == Key::Text {
        Cap::Text
    } else if w.on == 5 && w.in_from && w.key == Key::Username {
        Cap::User
    } else {
        Cap::None
    };
    n
}
// @end

// @harness contract_walk_str_start target=walk_str_start stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_walk_str_start() {
    let w: Walk = kani::any();
    let want = spec_walk_str_start(w);
    let got = walk_str_start(w);
    assert!(got == want);
}
// @end

// @fn walk_open
/// `{` or `[`: one level deeper, following the path to the fields of an update.
/// Contract: the result equals `spec_walk_open(w, is_obj)`. No panic for any input.
pub fn walk_open(w: Walk, is_obj: bool) -> Walk {
    // @body
    fn h_tmp_reset(mut n: Walk) -> Walk {
        n.cap = Cap::None;
        n.tmp_len = 0;
        n.tmp_over = false;
        n
    }
    let mut n = w;
    if w.depth >= MAX_DEPTH {
        n.err = true;
        return n;
    }
    let on_path = w.depth == w.on;
    let val = !w.expect_key;
    let enters = on_path
        && match w.on {
            0 => is_obj,
            1 => !is_obj && val && w.key == Key::Result,
            2 => is_obj,
            3 => is_obj && val && w.key == Key::Message,
            4 => is_obj && val && (w.key == Key::Chat || w.key == Key::From),
            _ => false,
        };
    n.depth = w.depth + 1;
    if enters {
        n.on = w.on + 1;
        if w.on == 2 {
            n.cur = RAW_EMPTY;
        }
        if w.on == 4 {
            n.in_from = w.key == Key::From;
        }
        n.expect_key = is_obj;
    }
    n.key = Key::None;
    h_tmp_reset(n)
}
// @end

// @harness contract_walk_open target=walk_open stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_walk_open() {
    let w: Walk = kani::any();
    let is_obj: bool = kani::any();
    let want = spec_walk_open(w, is_obj);
    let got = walk_open(w, is_obj);
    assert!(got == want);
}
// @end

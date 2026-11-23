// @fn walk_str_end
/// A string closes: classify a key, or store the captured text or username.
/// Contract: the result equals `spec_walk_str_end(w)`. No panic for any input.
pub fn walk_str_end(w: Walk) -> Walk {
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
    if !h_walk_in_obj(&w) || w.tmp_len as usize > TMP_CAP {
        return w;
    }
    let mut n = w;
    let s = &w.tmp[..w.tmp_len as usize];
    match w.cap {
        Cap::Key => n.key = if w.tmp_over { Key::Other } else { classify_key(w.on, w.in_from, s) },
        Cap::Text => {
            n.cur.has_text = true;
            n.cur.text_over = w.tmp_over;
            n.cur.text = [0; CMD_TEXT_CAP];
            n.cur.text_len = 0;
            if !w.tmp_over {
                let mut i = 0;
                while i < s.len() {
                    n.cur.text[i] = s[i];
                    i += 1;
                }
                n.cur.text_len = w.tmp_len;
            }
        }
        Cap::User => n.cur.who = name_from(s, w.tmp_over),
        _ => {}
    }
    h_tmp_reset(n)
}
// @end

// @harness contract_walk_str_end target=walk_str_end stubs=classify_key,name_from uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(66)]
#[kani::stub(classify_key, opaque::o_classify_key::f)]
#[kani::stub(spec_classify_key, opaque::o_classify_key::f)]
#[kani::stub(name_from, opaque::o_name_from::f)]
#[kani::stub(spec_name_from, opaque::o_name_from::f)]
fn contract_walk_str_end() {
    let w: Walk = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_walk_str_end(w);
    opaque::end();
    let got = walk_str_end(w);
    assert!(got == want);
}
// @end

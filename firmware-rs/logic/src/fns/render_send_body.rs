// @fn render_send_body
/// JSON body for sendMessage.
/// Contract: the result equals `spec_render_send_body(chat_id, text)`. No panic for any input.
pub fn render_send_body(chat_id: i64, text: &Text) -> Text {
    // @body
    if text.overflow || text.len as usize > TEXT_CAP {
        return Text { overflow: true, ..Text::EMPTY };
    }
    let t = text_push(Text::EMPTY, b"{\"chat_id\":");
    let t = text_push_i64(t, chat_id);
    let t = text_push(t, b",\"text\":\"");
    let t = text_push_json(t, &text.buf[..text.len as usize]);
    text_push(t, b"\"}")
}
// @end

// @harness contract_render_send_body target=render_send_body stubs=text_push,text_push_i64,text_push_json uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_i64, opaque::o_text_push_i64::f)]
#[kani::stub(spec_text_push_i64, opaque::o_text_push_i64::f)]
#[kani::stub(text_push_json, opaque::o_text_push_json::f)]
#[kani::stub(spec_text_push_json, opaque::o_text_push_json::f)]
fn contract_render_send_body() {
    let chat_id: i64 = kani::any();
    let text_val: Text = kani::any();
    let text: &Text = &text_val;
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_send_body(chat_id, text);
    opaque::end();
    let got = render_send_body(chat_id, text);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

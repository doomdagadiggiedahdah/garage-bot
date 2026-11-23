// @fn render_msg
/// The Telegram message text for a Msg.
/// Contract: the result equals `spec_render_msg(m, notify, version, d)`. No panic for any input.
pub fn render_msg(m: Msg, notify: &[u8], version: &[u8], d: Diag) -> Text {
    // @body
    let p = text_push;
    let t = Text::EMPTY;
    match m {
        Msg::Online => {
            let t = p(t, b"Garage door bot is online!\nVersion: ");
            let t = p(t, version);
            p(t, b"\n\nType 'help' for commands.")
        }
        Msg::CloseTimeout { who } => p(push_mention(t, &who), b"WARNING: Door did not close! It may be blocked."),
        Msg::OpenAlert { minutes } => {
            let t = p(t, notify);
            let t = p(t, b" ALERT: Garage door has been open for ");
            let t = text_push_u64(t, minutes);
            p(t, b" minutes!")
        }
        Msg::ClosedAfter(c) => render_closed_after(c),
        Msg::Reply(o) => render_reply(o, version, d),
    }
}
// @end

// @harness contract_render_msg target=render_msg stubs=text_push,push_mention,text_push_u64,render_closed_after,render_reply uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(push_mention, opaque::o_push_mention::f)]
#[kani::stub(spec_push_mention, opaque::o_push_mention::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(render_closed_after, opaque::o_render_closed_after::f)]
#[kani::stub(spec_render_closed_after, opaque::o_render_closed_after::f)]
#[kani::stub(render_reply, opaque::o_render_reply::f)]
#[kani::stub(spec_render_reply, opaque::o_render_reply::f)]
fn contract_render_msg() {
    let m: Msg = kani::any();
    let notify_buf: [u8; 8] = kani::any();
    let notify_len: usize = kani::any();
    kani::assume(notify_len <= 8);
    let notify: &[u8] = &notify_buf[..notify_len];
    let version_buf: [u8; 8] = kani::any();
    let version_len: usize = kani::any();
    kani::assume(version_len <= 8);
    let version: &[u8] = &version_buf[..version_len];
    let d: Diag = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_msg(m, notify, version, d);
    opaque::end();
    let got = render_msg(m, notify, version, d);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

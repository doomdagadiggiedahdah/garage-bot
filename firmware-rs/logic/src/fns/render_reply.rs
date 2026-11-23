// @fn render_reply
/// The text of a reply to a command.
/// Contract: the result equals `spec_render_reply(o, version, d)`. No panic for any input.
pub fn render_reply(o: Outgoing, version: &[u8], d: Diag) -> Text {
    // @body
    fn h_open_closed<'a>(open: bool) -> &'a [u8] {
        if open {
            b"OPEN"
        } else {
            b"CLOSED"
        }
    }
    let p = text_push;
    let t = Text::EMPTY;
    match o.reply {
        Reply::Status { open } => p(p(t, b"Door is "), h_open_closed(open)),
        Reply::AlreadyOpen => p(t, b"Door is already open!"),
        Reply::Opening => p(push_mention(t, &o.who), b"Opening door..."),
        Reply::AlreadyClosed => p(t, b"Door is already closed!"),
        Reply::Closing => p(push_mention(t, &o.who), b"Closing door..."),
        Reply::Muted => p(t, b"Alerts muted until door closes."),
        Reply::UpdateQueued => p(t, b"Update queued. It installs once pending commands are done."),
        Reply::Diag => render_diag(version, d),
        Reply::Version => p(p(t, b"GarageBot "), version),
    }
}
// @end

// @harness contract_render_reply target=render_reply stubs=text_push,push_mention,render_diag uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(push_mention, opaque::o_push_mention::f)]
#[kani::stub(spec_push_mention, opaque::o_push_mention::f)]
#[kani::stub(render_diag, opaque::o_render_diag::f)]
#[kani::stub(spec_render_diag, opaque::o_render_diag::f)]
fn contract_render_reply() {
    let o: Outgoing = kani::any();
    let version_buf: [u8; 8] = kani::any();
    let version_len: usize = kani::any();
    kani::assume(version_len <= 8);
    let version: &[u8] = &version_buf[..version_len];
    let d: Diag = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_reply(o, version, d);
    opaque::end();
    let got = render_reply(o, version, d);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

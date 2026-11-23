// @fn render_diag
/// The reply to the debug command.
/// Contract: the result equals `spec_render_diag(version, d)`. No panic for any input.
pub fn render_diag(version: &[u8], d: Diag) -> Text {
    // @body
    fn h_open_closed<'a>(open: bool) -> &'a [u8] {
        if open {
            b"OPEN"
        } else {
            b"CLOSED"
        }
    }
    let p = text_push;
    let (m, s) = div60(d.uptime_s);
    let t = p(Text::EMPTY, b"System Info:\nVersion: ");
    let t = p(t, version);
    let t = p(t, b"\nUptime: ");
    let t = text_push_u64(t, m);
    let t = p(t, b"m ");
    let t = text_push_u64(t, s);
    let t = p(t, b"s\nFree memory: ");
    let t = text_push_u64(t, d.free_mem as u64);
    let t = p(t, b" bytes\nLoop count: ");
    let t = text_push_u64(t, d.stats.loops);
    let t = p(t, b"\nDoor: ");
    let t = p(t, h_open_closed(d.door_open));
    let t = p(t, b"\nHTTP: ok=");
    let t = text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b" fail=");
    let t = text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b" sock_err=");
    text_push_u64(t, d.stats.sock_err as u64)
}
// @end

// @harness contract_render_diag target=render_diag stubs=div60,text_push,text_push_u64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(div60, opaque::o_div60::f)]
#[kani::stub(spec_div60, opaque::o_div60::f)]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
fn contract_render_diag() {
    let version_buf: [u8; 8] = kani::any();
    let version_len: usize = kani::any();
    kani::assume(version_len <= 8);
    let version: &[u8] = &version_buf[..version_len];
    let d: Diag = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_diag(version, d);
    opaque::end();
    let got = render_diag(version, d);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

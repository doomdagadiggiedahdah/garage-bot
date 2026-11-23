// @fn render_heartbeat
/// The HEARTBEAT log message.
/// Contract: the result equals `spec_render_heartbeat(d)`. No panic for any input.
pub fn render_heartbeat(d: Diag) -> Text {
    // @body
    fn h_open_closed<'a>(open: bool) -> &'a [u8] {
        if open {
            b"OPEN"
        } else {
            b"CLOSED"
        }
    }
    let p = text_push;
    let t = p(Text::EMPTY, b"HEARTBEAT: door=");
    let t = p(t, h_open_closed(d.door_open));
    let t = p(t, b", uptime=");
    let t = text_push_u64(t, div60(d.uptime_s).0);
    let t = p(t, b"m, loops=");
    let t = text_push_u64(t, d.stats.loops);
    let t = p(t, b", mem=");
    let t = text_push_u64(t, d.free_mem as u64);
    let t = p(t, b", rssi=");
    let t = text_push_i64(t, d.rssi as i64);
    let t = p(t, b", http_ok=");
    let t = text_push_u64(t, d.stats.http_ok as u64);
    let t = p(t, b", http_fail=");
    let t = text_push_u64(t, d.stats.http_fail as u64);
    let t = p(t, b", sock_err=");
    text_push_u64(t, d.stats.sock_err as u64)
}
// @end

// @harness contract_render_heartbeat target=render_heartbeat stubs=text_push,div60,text_push_u64,text_push_i64 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(div60, opaque::o_div60::f)]
#[kani::stub(spec_div60, opaque::o_div60::f)]
#[kani::stub(text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(spec_text_push_u64, opaque::o_text_push_u64::f)]
#[kani::stub(text_push_i64, opaque::o_text_push_i64::f)]
#[kani::stub(spec_text_push_i64, opaque::o_text_push_i64::f)]
fn contract_render_heartbeat() {
    let d: Diag = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_heartbeat(d);
    opaque::end();
    let got = render_heartbeat(d);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

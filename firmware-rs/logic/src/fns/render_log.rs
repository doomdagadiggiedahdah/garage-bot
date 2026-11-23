// @fn render_log
/// One log line in the format the log server parses.
/// Contract: the result equals `spec_render_log(uptime_s, level, free_mem, msg)`. No panic for any input.
pub fn render_log(uptime_s: u64, level: Level, free_mem: u32, msg: &[u8]) -> Text {
    // @body
    let t = text_push(Text::EMPTY, b"[");
    let t = text_push_pad6(t, uptime_s);
    let t = text_push(t, b"s] [");
    let lv: &[u8] = match level {
        Level::Info => b" INFO",
        Level::Warn => b" WARN",
        Level::Error => b"ERROR",
    };
    let t = text_push(t, lv);
    let t = text_push(t, b"] [mem:");
    let t = text_push_pad6(t, free_mem as u64);
    let t = text_push(t, b"] ");
    text_push(t, msg)
}
// @end

// @harness contract_render_log target=render_log stubs=text_push,text_push_pad6 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
#[kani::stub(text_push_pad6, opaque::w_text_push_pad6)]
#[kani::stub(spec_text_push_padded, opaque::o_text_push_padded::f)]
fn contract_render_log() {
    let uptime_s: u64 = kani::any();
    let level: Level = kani::any();
    let free_mem: u32 = kani::any();
    let msg_buf: [u8; 8] = kani::any();
    let msg_len: usize = kani::any();
    kani::assume(msg_len <= 8);
    let msg: &[u8] = &msg_buf[..msg_len];
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_render_log(uptime_s, level, free_mem, msg);
    opaque::end();
    let got = render_log(uptime_s, level, free_mem, msg);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end

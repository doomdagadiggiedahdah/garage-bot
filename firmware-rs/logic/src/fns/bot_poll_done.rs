// @fn bot_poll_done
/// The getUpdates exchange is over.
/// Contract: the result equals `spec_bot_poll_done(b, http_ok, date_hdr, t_req_ms, now)`. No panic for any input.
pub fn bot_poll_done(b: Bot, http_ok: bool, date_hdr: &[u8], t_req_ms: u64, now: u64) -> Bot {
    // @body
    fn h_inbox_ok(q: &Inbox) -> bool {
        (q.head as usize) < INBOX_CAP && q.len as usize <= INBOX_CAP
    }
    if b.core.batch_left > 0 {
        return b;
    }
    let ok = http_ok && scan_finish(b.scan) && h_inbox_ok(&b.inbox);
    let mut n = b;
    if !ok {
        n.inbox = INBOX_EMPTY;
    }
    n.scan = SCAN_START;
    let p = PollResult { ok, count: n.inbox.len as u32, date_unix: parse_http_date(date_hdr), t_req_ms };
    n.core = on_poll(b.core, p, now);
    n
}
// @end

// @harness contract_bot_poll_done target=bot_poll_done stubs=scan_finish,parse_http_date,on_poll uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(scan_finish, opaque::o_scan_finish::f)]
#[kani::stub(spec_scan_finish, opaque::o_scan_finish::f)]
#[kani::stub(parse_http_date, opaque::o_http_date::f)]
#[kani::stub(spec_http_date, opaque::o_http_date::f)]
#[kani::stub(on_poll, opaque::o_on_poll::f)]
#[kani::stub(spec_on_poll, opaque::o_on_poll::f)]
fn contract_bot_poll_done() {
    let b: Bot = kani::any();
    let http_ok: bool = kani::any();
    let date_hdr_buf: [u8; 30] = kani::any();
    let date_hdr_len: usize = kani::any();
    kani::assume(date_hdr_len <= 30);
    let date_hdr: &[u8] = &date_hdr_buf[..date_hdr_len];
    let t_req_ms: u64 = kani::any();
    let now: u64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_bot_poll_done(b, http_ok, date_hdr, t_req_ms, now);
    opaque::end();
    let got = bot_poll_done(b, http_ok, date_hdr, t_req_ms, now);
    assert!(got == want);
}
// @end

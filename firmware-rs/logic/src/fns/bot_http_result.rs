// @fn bot_http_result
/// Count an HTTP call for the diagnostics.
/// Contract: the result equals `spec_bot_http_result(b, ok, sock_err)`. No panic for any input.
pub fn bot_http_result(b: Bot, ok: bool, sock_err: bool) -> Bot {
    // @body
    let mut b = b;
    if ok {
        b.stats.http_ok = b.stats.http_ok.saturating_add(1);
    } else {
        b.stats.http_fail = b.stats.http_fail.saturating_add(1);
    }
    if sock_err {
        b.stats.sock_err = b.stats.sock_err.saturating_add(1);
    }
    b
}
// @end

// @harness contract_bot_http_result target=bot_http_result stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_bot_http_result() {
    let b: Bot = kani::any();
    let ok: bool = kani::any();
    let sock_err: bool = kani::any();
    let want = spec_bot_http_result(b, ok, sock_err);
    let got = bot_http_result(b, ok, sock_err);
    assert!(got == want);
}
// @end

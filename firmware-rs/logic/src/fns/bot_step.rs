// @fn bot_step
/// One loop pass. The shell calls this once per pass, right after feeding the watchdog.
/// The update timer comes first: at boot and every OTA_CHECK_INTERVAL_MS it raises update_pending.
/// Contract: the result equals `spec_bot_step(b, i)`. No panic for any input.
pub fn bot_step(b: Bot, i: StepIn) -> BotStep {
    // @body
    let due = interval_due(b.ota_last_check, i.now_ms, OTA_CHECK_INTERVAL_MS);
    let mut core = b.core;
    if due {
        core.update_pending = true;
    }
    let o = step(core, i);
    let mut n = b;
    if due {
        n.ota_last_check = Some(i.now_ms);
    }
    n.core = o.state;
    n.stats.loops = b.stats.loops.saturating_add(1);
    if let Op::Poll { .. } = o.op {
        n.scan = SCAN_START;
        n.inbox = INBOX_EMPTY;
    }
    BotStep { bot: n, op: o.op, heartbeat: o.heartbeat }
}
// @end

// @harness contract_bot_step target=bot_step stubs=interval_due,step uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(interval_due, opaque::o_due::f)]
#[kani::stub(spec_due, opaque::o_due::f)]
#[kani::stub(step, opaque::o_step::f)]
#[kani::stub(spec_step, opaque::o_step::f)]
fn contract_bot_step() {
    let b: Bot = kani::any();
    let i: StepIn = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_bot_step(b, i);
    opaque::end();
    let got = bot_step(b, i);
    assert!(got == want);
}
// @end

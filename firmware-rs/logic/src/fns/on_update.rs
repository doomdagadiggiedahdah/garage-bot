// @fn on_update
/// Process one update of the batch. bool = press the relay now.
/// Contract: the result equals `spec_on_update(s, u, now, door_open)`. No panic for any input.
pub fn on_update(s: State, u: UpdateIn, now: u64, door_open: bool) -> (State, bool) {
    // @body
    if s.batch_left == 0 || s.pend_reply.is_some() {
        return (s, false);
    }
    let mut n = s;
    n.batch_left = s.batch_left - 1;
    let id_ok = u.update_id < MAX_UPDATE_ID && u.update_id >= s.next_offset;
    if id_ok {
        n.next_offset = u.update_id + 1;
    }
    if !(id_ok && u.authorized) {
        return (n, false);
    }
    let d = decide(u.cmd, judge_age(s.anchor, s.boot_unix, now, u.date), door_open);
    n.pend_reply = match d.reply {
        Some(r) => Some(Outgoing { who: u.who, reply: r }),
        None => None,
    };
    if d.arm_close {
        n.door.close = Some(CloseReq { at: now, who: u.who, alerted: false });
    }
    if d.mute {
        n.door.muted = true;
    }
    if d.queue_update {
        n.update_pending = true;
    }
    (n, d.press)
}
// @end

// @harness contract_on_update target=on_update stubs=judge_age,decide uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(judge_age, opaque::o_age::f)]
#[kani::stub(spec_age, opaque::o_age::f)]
#[kani::stub(decide, opaque::o_decide::f)]
#[kani::stub(spec_decide, opaque::o_decide::f)]
fn contract_on_update() {
    let s: State = kani::any();
    let u: UpdateIn = kani::any();
    let now: u64 = kani::any();
    let door_open: bool = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_on_update(s, u, now, door_open);
    opaque::end();
    let got = on_update(s, u, now, door_open);
    assert!(got == want);
}
// @end

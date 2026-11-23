// @fn bot_next_update
/// Act on the next queued update. bool = press the relay now.
/// Contract: the result equals `spec_bot_next_update(b, chat_id, now, door_open)`. No panic for any input.
pub fn bot_next_update(b: Bot, chat_id: i64, now: u64, door_open: bool) -> (Bot, bool) {
    // @body
    if b.core.batch_left == 0 || b.core.pend_reply.is_some() {
        return (b, false);
    }
    let (inbox, item) = inbox_pop(b.inbox);
    let mut n = b;
    n.inbox = inbox;
    match item {
        None => {
            n.core.batch_left = 0;
            (n, false)
        }
        Some(raw) => match to_update_in(raw, chat_id) {
            None => {
                n.core.batch_left = b.core.batch_left - 1;
                (n, false)
            }
            Some(u) => {
                let (core, press) = on_update(b.core, u, now, door_open);
                n.core = core;
                (n, press)
            }
        },
    }
}
// @end

// @harness contract_bot_next_update target=bot_next_update stubs=inbox_pop,to_update_in,on_update uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(inbox_pop, opaque::o_inbox_pop::f)]
#[kani::stub(spec_inbox_pop, opaque::o_inbox_pop::f)]
#[kani::stub(to_update_in, opaque::o_to_update_in::f)]
#[kani::stub(spec_to_update_in, opaque::o_to_update_in::f)]
#[kani::stub(on_update, opaque::o_on_update::f)]
#[kani::stub(spec_on_update, opaque::o_on_update::f)]
fn contract_bot_next_update() {
    let b: Bot = kani::any();
    let chat_id: i64 = kani::any();
    let now: u64 = kani::any();
    let door_open: bool = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_bot_next_update(b, chat_id, now, door_open);
    opaque::end();
    let got = bot_next_update(b, chat_id, now, door_open);
    assert!(got == want);
}
// @end

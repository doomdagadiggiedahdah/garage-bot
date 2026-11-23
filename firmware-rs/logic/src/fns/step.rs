// @fn step
/// One loop pass of the core.
/// Contract: the result equals `spec_step(s, i)`. No panic for any input.
pub fn step(s: State, i: StepIn) -> StepOut {
    // @body
    let now = i.now_ms;
    let overrun = overrun(s.last_step, now);
    let (door, ev) = door_tick(s.door, now, i.door_open);
    let mut n = s;
    n.last_step = Some(now);
    n.door = door;
    n = queue_door_events(n, ev, i.door_open);
    n.wifi_down_since = wifi_track(s.wifi_down_since, now, i.wifi_up);
    let hb = interval_due(s.last_heartbeat, now, HEARTBEAT_INTERVAL_MS);
    if hb {
        n.last_heartbeat = Some(now);
    }
    let op = pick_op(n, now, overrun, i.wifi_up);
    let n = op_done(n, op, now);
    StepOut { state: n, op, heartbeat: if hb { Some(uptime_s(now)) } else { None } }
}
// @end

// @harness contract_step target=step stubs=overrun,door_tick,queue_door_events,wifi_track,interval_due,pick_op,op_done,uptime_s uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(overrun, opaque::o_overrun::f)]
#[kani::stub(spec_overrun, opaque::o_overrun::f)]
#[kani::stub(door_tick, opaque::o_door::f)]
#[kani::stub(spec_door, opaque::o_door::f)]
#[kani::stub(queue_door_events, opaque::o_queue_door_events::f)]
#[kani::stub(spec_queue_door_events, opaque::o_queue_door_events::f)]
#[kani::stub(wifi_track, opaque::o_wifi_track::f)]
#[kani::stub(spec_wifi_track, opaque::o_wifi_track::f)]
#[kani::stub(interval_due, opaque::o_due::f)]
#[kani::stub(spec_due, opaque::o_due::f)]
#[kani::stub(pick_op, opaque::o_pick_op::f)]
#[kani::stub(spec_pick_op, opaque::o_pick_op::f)]
#[kani::stub(op_done, opaque::o_op_done::f)]
#[kani::stub(spec_op_done, opaque::o_op_done::f)]
#[kani::stub(uptime_s, opaque::o_uptime_s::f)]
#[kani::stub(spec_uptime_s, opaque::o_uptime_s::f)]
fn contract_step() {
    let s: State = kani::any();
    let i: StepIn = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_step(s, i);
    opaque::end();
    let got = step(s, i);
    assert!(got == want);
}
// @end

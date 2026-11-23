// @fn queue_door_events
/// Put this pass's door events into the pending slots; a closed door first drops the open-door ones.
/// Contract: the result equals `spec_queue_door_events(s, ev, door_open)`. No panic for any input.
pub fn queue_door_events(s: State, ev: DoorEvents, door_open: bool) -> State {
    // @body
    let mut n = s;
    if !door_open {
        // nothing about an open door is worth saying once it is closed
        n.pend_open_alert = None;
        n.pend_close_timeout = None;
    }
    if let Some(m) = ev.open_alert {
        n.pend_open_alert = Some(m);
    }
    if let Some(w) = ev.close_timeout {
        n.pend_close_timeout = Some(w);
    }
    if let Some(c) = ev.closed_notice {
        n.pend_closed_notice = Some(c);
    }
    n
}
// @end

// @harness contract_queue_door_events target=queue_door_events stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_queue_door_events() {
    let s: State = kani::any();
    let ev: DoorEvents = kani::any();
    let door_open: bool = kani::any();
    let want = spec_queue_door_events(s, ev, door_open);
    let got = queue_door_events(s, ev, door_open);
    assert!(got == want);
}
// @end

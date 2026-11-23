// @fn decide
/// What a command does, given its age verdict and the door.
/// Contract: the result equals `spec_decide(cmd, age, door_open)`. No panic for any input.
pub fn decide(cmd: Cmd, age: Age, door_open: bool) -> Decision {
    // @body
    let none = Decision { press: false, reply: None, arm_close: false, mute: false, queue_update: false };
    let say = |r: Reply| Decision { reply: Some(r), ..none };
    match cmd {
        Cmd::Unknown => none,
        Cmd::Status => say(Reply::Status { open: door_open }),
        Cmd::Diag => say(Reply::Diag),
        Cmd::Version => say(Reply::Version),
        Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Update => match age {
            // Not fresh: dropped silently (no press, no reply).
            Age::Unknown | Age::PreBoot | Age::Stale(_) => none,
            Age::Fresh(_) => match cmd {
                Cmd::Open => {
                    if door_open {
                        say(Reply::AlreadyOpen)
                    } else {
                        Decision { press: true, ..say(Reply::Opening) }
                    }
                }
                Cmd::Close => {
                    if door_open {
                        Decision { press: true, arm_close: true, ..say(Reply::Closing) }
                    } else {
                        say(Reply::AlreadyClosed)
                    }
                }
                Cmd::Silence => Decision { mute: true, ..say(Reply::Muted) },
                _ => Decision { queue_update: true, ..say(Reply::UpdateQueued) },
            },
        },
    }
}
// @end

// @harness contract_decide target=decide stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_decide() {
    let cmd: Cmd = kani::any();
    let age: Age = kani::any();
    let door_open: bool = kani::any();
    let want = spec_decide(cmd, age, door_open);
    let got = decide(cmd, age, door_open);
    assert!(got == want);
}
// @end

// @fn inbox_push
/// Queue a parsed update; a full inbox is unchanged.
/// Contract: the result equals `spec_inbox_push(q, u)`. No panic for any input.
pub fn inbox_push(q: Inbox, u: RawUpdate) -> Inbox {
    // @body
    if (q.head as usize) >= INBOX_CAP || (q.len as usize) >= INBOX_CAP {
        return q;
    }
    let mut n = q;
    n.items[(q.head as usize + q.len as usize) % INBOX_CAP] = u;
    n.len = q.len + 1;
    n
}
// @end

// @harness contract_inbox_push target=inbox_push stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(10)]
fn contract_inbox_push() {
    let q: Inbox = kani::any();
    let u: RawUpdate = kani::any();
    let want = spec_inbox_push(q, u);
    let got = inbox_push(q, u);
    assert!(got == want);
}
// @end

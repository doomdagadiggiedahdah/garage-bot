// @fn inbox_pop
/// Take the oldest parsed update.
/// Contract: the result equals `spec_inbox_pop(q)`. No panic for any input.
pub fn inbox_pop(q: Inbox) -> (Inbox, Option<RawUpdate>) {
    // @body
    let mut q = q;
    if (q.head as usize) >= INBOX_CAP || (q.len as usize) > INBOX_CAP || q.len == 0 {
        return (q, None);
    }
    let item = q.items[q.head as usize];
    q.head = ((q.head as usize + 1) % INBOX_CAP) as u8;
    q.len -= 1;
    (q, Some(item))
}
// @end

// @harness contract_inbox_pop target=inbox_pop stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(10)]
fn contract_inbox_pop() {
    let q: Inbox = kani::any();
    let want = spec_inbox_pop(q);
    let got = inbox_pop(q);
    assert!(got == want);
}
// @end
